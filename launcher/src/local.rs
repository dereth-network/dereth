//! The worlds already running on this computer that the launcher did not make: Empyrean, ACE and
//! GDLE servers, found by their processes ([`dereth_launch::local`]).
//!
//! Every few seconds while Host a world is open, the launcher lists the processes whose program
//! could be a server's, reads each one's command line, the UDP ports it listens on and what
//! started it (a Windows service, a systemd service, launchd, or a person), and asks it how it is:
//! Empyrean with the status ping (its name, version and players), ACE and GDLE with the
//! server-tracker login (up or down). Its configuration, data and log are where its own layout
//! puts them.
//!
//! **Control.** The launcher never stops a process it did not start. An Empyrean it has seen
//! running from a folder, not as a service, is remembered (`local-worlds.json` in the data folder);
//! once it stops, the launcher can start it again from that folder, as its own child, and then
//! stop it (its console's `exit`, which saves first; killed only when the person asks a second
//! time) and pass it console commands. A world a service manager runs says which, and is started
//! and stopped there.
//!
//! **The admin channel.** An Empyrean that has one ([`dereth_launch::local::admin_channel`]) is
//! connected to as soon as it is found: its log comes over the channel (the recent lines, then each
//! new one), its console takes the page's commands, and Stop asks it to save and stop there. A world
//! a service manager runs is started and stopped through the service manager when the launcher may
//! ([`crate::services`]), its channel or not, so a service that starts its world again when it
//! exits cannot undo a Stop; when it may not, Stop goes over the channel, or the page shows the
//! command.

use crate::services::{Control, Services};
use dereth_launch::local::{self, Disk, Files, Layout, Process, Software};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::State;

/// How often a world is asked how it is: Empyrean answers a few status pings a minute from one
/// address, and a launcher left open on Host a world asks no more than that.
const PROBE_EVERY: Duration = Duration::from_secs(30);

/// How soon the page may ask for another look.
const SCAN_EVERY: Duration = Duration::from_secs(5);

/// The remembered worlds' file, in the data folder.
const KNOWN_FILE: &str = "local-worlds.json";

/// The lines of a log the page shows.
const TAIL_LINES: usize = 200;

/// What started a server, and so where it is started and stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Manager {
    /// A Windows service, by its name.
    WindowsService { name: String },
    /// A systemd service; `user` for the person's own (`systemctl --user`).
    Systemd { unit: String, user: bool },
    /// launchd, which started it on a Mac.
    Launchd,
    /// A person, or a program other than the launcher.
    Elsewhere,
    /// This launcher.
    Launcher,
}

impl Manager {
    fn service(&self) -> Option<String> {
        match self {
            Self::WindowsService { name } => Some(name.clone()),
            Self::Systemd { unit, .. } => Some(unit.clone()),
            Self::Launchd => Some("launchd".into()),
            Self::Elsewhere | Self::Launcher => None,
        }
    }
}

/// A server process found running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub process: Process,
    /// When it started (seconds since 1970), when the system says.
    pub started: Option<u64>,
    /// The UDP addresses and ports it listens on.
    pub ports: Vec<(IpAddr, u16)>,
    pub manager: Manager,
}

/// What the launcher asks of the system: the server processes running.
pub trait System {
    /// Every running process whose program could be a server's, described.
    fn servers(&self) -> Vec<Running>;
}

/// How a world answered when asked.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Answer {
    /// `online`, `starting`, `offline` or `unknown`.
    pub state: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub players: Option<u32>,
}

/// An Empyrean the launcher has seen running from a folder, and so can start from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Known {
    pub key: String,
    pub program: PathBuf,
    pub config: PathBuf,
    #[serde(default)]
    pub name: Option<String>,
    /// The service it ran as when last seen, if it did: then it is started there, not here.
    #[serde(default)]
    pub service: Option<String>,
    /// What ran it when last seen, when a service manager did.
    #[serde(default)]
    pub manager: Option<Manager>,
}

/// One world on the page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct View {
    pub key: String,
    pub software: Option<Software>,
    pub name: String,
    pub running: bool,
    pub pid: Option<u32>,
    /// As it answered: `online`, `starting`, `offline` or `unknown`.
    pub state: String,
    pub version: Option<String>,
    pub players: Option<u32>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub started: Option<u64>,
    pub manager: Option<Manager>,
    /// The launcher started it: it can stop it and pass it commands.
    pub ours: bool,
    /// A stop has been asked for; asking again kills it.
    pub stopping: bool,
    /// Why the launcher cannot start it, when it is stopped; none when it can.
    pub cannot_start: Option<String>,
    pub folder: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub data: Option<PathBuf>,
    pub dats: Option<PathBuf>,
    pub log: Option<PathBuf>,
    pub logs: Vec<String>,
    pub open_accounts: Option<bool>,
    pub message: Option<String>,
    /// The launcher is connected to its admin channel: its log comes over it, and it takes
    /// commands.
    pub channel: bool,
    /// Its slug on the Dereth Network, as its configuration gives it.
    pub federation_slug: Option<String>,
    /// That slug is on the Dereth Network's list.
    pub listed: bool,
    /// What the launcher can do about the service manager that runs it, when one does.
    pub control: Option<Control>,
}

/// A connection to a world's admin channel.
struct Channel {
    pid: u32,
    writer: Mutex<TcpStream>,
    lines: Arc<Mutex<VecDeque<String>>>,
    alive: Arc<AtomicBool>,
    stopping: bool,
}

impl Channel {
    fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Sends one console command.
    fn send(&self, line: &str) -> std::io::Result<()> {
        let mut w = self
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        writeln!(w, "{line}").and_then(|()| w.flush())
    }

    /// Adds a line to what the page shows of the world's console.
    fn show(&self, line: String) {
        push_line(&self.lines, line);
    }
}

/// Keeps `line` in `log`, the oldest dropped past [`KEPT_LINES`].
fn push_line(log: &Mutex<VecDeque<String>>, line: String) {
    let mut log = log
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if log.len() == KEPT_LINES {
        log.pop_front();
    }
    log.push_back(line);
}

/// The lines of a world's console kept for the page.
const KEPT_LINES: usize = 300;

/// How long connecting to an admin channel may take: it is on this computer.
const CHANNEL_CONNECT: Duration = Duration::from_secs(2);

/// Connects to `channel` and gives its secret; the world's lines then come into the connection's
/// own log, credentials hidden, until either side ends it.
fn connect_channel(channel: &local::AdminChannel, pid: u32) -> Option<Channel> {
    let mut stream = TcpStream::connect_timeout(&channel.address, CHANNEL_CONNECT).ok()?;
    stream.set_write_timeout(Some(CHANNEL_CONNECT)).ok()?;
    writeln!(stream, "{}", channel.secret).ok()?;
    let read_half = stream.try_clone().ok()?;
    let lines = Arc::new(Mutex::new(VecDeque::new()));
    let alive = Arc::new(AtomicBool::new(true));
    {
        let lines = Arc::clone(&lines);
        let alive = Arc::clone(&alive);
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(read_half);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if let Some(local::ChannelLine::Console(text)) = local::channel_line(&line) {
                    push_line(&lines, crate::host_config::redact(&text));
                }
            }
            alive.store(false, Ordering::SeqCst);
        });
    }
    Some(Channel {
        pid,
        writer: Mutex::new(stream),
        lines,
        alive,
        stopping: false,
    })
}

/// A world the launcher started from a folder it knows.
struct Started {
    key: String,
    child: Child,
    stdin: Option<ChildStdin>,
    log: Arc<Mutex<VecDeque<String>>>,
    stopping: bool,
    started: u64,
}

pub struct Store {
    data: PathBuf,
    views: Vec<View>,
    known: Vec<Known>,
    children: Vec<Started>,
    scanning: bool,
    scanned: Option<Instant>,
    answers: HashMap<String, (Instant, Answer)>,
    messages: HashMap<String, String>,
    /// The admin channels connected to, by world.
    channels: HashMap<String, Channel>,
    services: Arc<dyn Services>,
}

pub type Shared = Arc<Mutex<Store>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Store> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The store, over the launcher's data folder.
pub fn new(data: PathBuf) -> Shared {
    with_services(data, Arc::new(crate::services::System))
}

/// The store, over the launcher's data folder, with `services` for the service managers.
pub fn with_services(data: PathBuf, services: Arc<dyn Services>) -> Shared {
    let known = std::fs::read(data.join(KNOWN_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    Arc::new(Mutex::new(Store {
        data,
        views: Vec::new(),
        known,
        children: Vec::new(),
        scanning: false,
        scanned: None,
        answers: HashMap::new(),
        messages: HashMap::new(),
        channels: HashMap::new(),
        services,
    }))
}

/// A world's key: what it is and where its configuration (else its program) is, the same from one
/// look to the next.
fn key_of(software: Software, layout: &Layout, process: &Process) -> String {
    let at = layout.config.as_ref().unwrap_or(&process.program);
    let at = at.to_string_lossy();
    let at = if cfg!(windows) {
        at.to_lowercase()
    } else {
        at.into_owned()
    };
    format!("{}:{at}", software.title().to_ascii_lowercase())
}

/// The last `TAIL_LINES` lines of a log file, credentials hidden.
fn tail_of(path: &Path) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let length = file.metadata().map_or(0, |m| m.len());
    use std::io::Seek;
    let _ = file.seek(std::io::SeekFrom::Start(length.saturating_sub(64 * 1024)));
    let mut bytes = Vec::new();
    let _ = file.take(64 * 1024).read_to_end(&mut bytes);
    local::tail(&String::from_utf8_lossy(&bytes), TAIL_LINES)
        .iter()
        .map(|l| crate::host_config::redact(l))
        .collect()
}

/// What the page shows of the servers `system` finds and the worlds remembered in `known`, which
/// gains the Empyrean worlds found running from a folder. A server whose configuration is in
/// `own` (the launcher's own worlds' folder) is the launcher's, and shown there instead. `ask`
/// asks a world how it is.
pub fn assemble(
    system: &dyn System,
    files: &dyn Files,
    own: &Path,
    known: &mut Vec<Known>,
    services: &dyn Services,
    ask: &mut dyn FnMut(&str, Software, IpAddr, u16) -> Answer,
) -> Vec<View> {
    let mut views = Vec::new();
    let mut seen = HashSet::new();
    for running in system.servers() {
        let Some(software) = local::recognize(&running.process) else {
            continue;
        };
        let layout = local::layout(software, &running.process, files);
        if layout.config.as_deref().is_some_and(|c| c.starts_with(own)) {
            continue;
        }
        let key = key_of(software, &layout, &running.process);
        if !seen.insert(key.clone()) {
            continue;
        }
        let listening: Vec<u16> = running.ports.iter().map(|(_, p)| *p).collect();
        let port = local::game_port(layout.port, &listening);
        let answer = port.map_or_else(
            || Answer {
                state: "unknown".into(),
                ..Answer::default()
            },
            |port| ask(&key, software, local::ask_at(layout.host.as_deref()), port),
        );
        if software == Software::Empyrean {
            if let Some(config) = &layout.config {
                let entry = Known {
                    key: key.clone(),
                    program: running.process.program.clone(),
                    config: config.clone(),
                    name: answer.name.clone().or_else(|| layout.name.clone()),
                    service: running.manager.service(),
                    manager: running.manager.service().map(|_| running.manager.clone()),
                };
                match known.iter_mut().find(|k| k.key == key) {
                    Some(k) => *k = entry,
                    None => known.push(entry),
                }
            }
        }
        views.push(View {
            name: answer
                .name
                .clone()
                .or_else(|| layout.name.clone())
                .or_else(|| folder_name(layout.folder.as_deref()))
                .unwrap_or_else(|| software.title().to_owned()),
            software: Some(software),
            running: true,
            pid: Some(running.process.pid),
            state: answer.state,
            version: answer.version,
            players: answer.players,
            host: layout.host.clone(),
            port,
            started: running.started,
            manager: Some(running.manager.clone()),
            control: running
                .manager
                .service()
                .map(|_| crate::services::control(&running.manager, services)),
            logs: layout.log.as_deref().map(tail_of).unwrap_or_default(),
            federation_slug: layout.federation_slug.clone(),
            folder: layout.folder,
            config: layout.config,
            data: layout.data,
            dats: layout.dats,
            log: layout.log,
            open_accounts: layout.open_accounts,
            key,
            ..View::default()
        });
    }
    for k in known.iter() {
        if seen.contains(&k.key) {
            continue;
        }
        let process = Process {
            program: k.program.clone(),
            args: vec![
                k.program.to_string_lossy().into_owned(),
                "--config".into(),
                k.config.to_string_lossy().into_owned(),
            ],
            ..Process::default()
        };
        let layout = local::layout(Software::Empyrean, &process, files);
        let control = k
            .manager
            .as_ref()
            .map(|m| crate::services::control(m, services));
        let cannot_start = if let Some(service) = &k.service {
            match &control {
                Some(c) if c.can_control => None,
                Some(Control {
                    start_command: Some(command),
                    ..
                }) => Some(format!(
                    "It runs as the service {service}. Start it with: {command}"
                )),
                _ => Some(format!("It ran as the service {service}. Start it there.")),
            }
        } else if !files.is_file(&k.program) || !files.is_file(&k.config) {
            Some("Its program or its configuration is no longer there.".into())
        } else if layout.console == Some(false) {
            Some(
                "Its console is off (interactive_console = false), so the launcher could not stop it cleanly."
                    .into(),
            )
        } else {
            None
        };
        views.push(View {
            key: k.key.clone(),
            software: Some(Software::Empyrean),
            name: k
                .name
                .clone()
                .or_else(|| layout.name.clone())
                .unwrap_or_else(|| "Empyrean".into()),
            state: "offline".into(),
            host: layout.host.clone(),
            port: layout.port,
            manager: k.manager.clone(),
            control,
            cannot_start,
            logs: layout.log.as_deref().map(tail_of).unwrap_or_default(),
            federation_slug: layout.federation_slug.clone(),
            folder: layout.folder,
            config: layout.config,
            data: layout.data,
            dats: layout.dats,
            log: layout.log,
            open_accounts: layout.open_accounts,
            ..View::default()
        });
    }
    views
}

fn folder_name(folder: Option<&Path>) -> Option<String> {
    folder?
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

/// How a server answers at `ip`:`port`: Empyrean's status ping, else the server-tracker login.
fn ask_world(software: Software, ip: IpAddr, port: u16) -> Answer {
    let host = ip.to_string();
    if software == Software::Empyrean {
        if let Some(live) = crate::backend::ping_status(&host, port) {
            return Answer {
                state: match live.state {
                    dereth_launch::world::WorldState::Online
                    | dereth_launch::world::WorldState::High => "online",
                    dereth_launch::world::WorldState::Starting => "starting",
                    dereth_launch::world::WorldState::Offline => "offline",
                    dereth_launch::world::WorldState::Unknown => "unknown",
                }
                .into(),
                name: live.world_name,
                version: live.version,
                players: live.players,
            };
        }
    }
    Answer {
        state: if crate::backend::probe_host(&host, port) {
            "online"
        } else {
            "unknown"
        }
        .into(),
        ..Answer::default()
    }
}

/// The views, with what the launcher knows of the worlds it started.
pub fn views(shared: &Shared) -> Vec<View> {
    let mut store = lock(shared);
    store
        .children
        .retain_mut(|c| !matches!(c.child.try_wait(), Ok(Some(_))));
    let started: HashMap<String, (u32, bool, u64, Vec<String>)> = store
        .children
        .iter()
        .map(|c| {
            let lines = c
                .log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .iter()
                .cloned()
                .collect();
            (c.key.clone(), (c.child.id(), c.stopping, c.started, lines))
        })
        .collect();
    store.channels.retain(|_, c| c.alive());
    let mut views = store.views.clone();
    for v in &mut views {
        v.message = store.messages.get(&v.key).cloned();
        if let Some(c) = store.channels.get(&v.key) {
            if v.running && Some(c.pid) == v.pid {
                v.channel = true;
                v.stopping = c.stopping;
                v.logs = c
                    .lines
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .iter()
                    .cloned()
                    .collect();
            }
        }
        if let Some((pid, stopping, at, lines)) = started.get(&v.key) {
            v.ours = true;
            v.running = true;
            v.pid = Some(*pid);
            v.stopping = *stopping;
            v.started = v.started.or(Some(*at));
            v.manager = Some(Manager::Launcher);
            v.cannot_start = None;
            v.logs.clone_from(lines);
        }
    }
    views
}

/// Look at the computer's servers again, unless a look is under way or was taken a moment ago.
pub fn scan(shared: &Shared, force: bool) {
    {
        let mut store = lock(shared);
        if store.scanning || (!force && store.scanned.is_some_and(|at| at.elapsed() < SCAN_EVERY)) {
            return;
        }
        store.scanning = true;
    }
    let shared = shared.clone();
    std::thread::spawn(move || {
        let (own, mut known, mut answers, data, services) = {
            let store = lock(&shared);
            (
                store.data.join("hosts"),
                store.known.clone(),
                store.answers.clone(),
                store.data.clone(),
                Arc::clone(&store.services),
            )
        };
        let before = known.clone();
        let mut ask = |key: &str, software: Software, ip: IpAddr, port: u16| -> Answer {
            if let Some((at, answer)) = answers.get(key) {
                if at.elapsed() < PROBE_EVERY {
                    return answer.clone();
                }
            }
            let answer = ask_world(software, ip, port);
            answers.insert(key.to_owned(), (Instant::now(), answer.clone()));
            answer
        };
        let views = assemble(&OsSystem, &Disk, &own, &mut known, &*services, &mut ask);
        if known != before {
            let _ = crate::host_runtime::write_json(&data.join(KNOWN_FILE), &known);
        }
        connect_channels(&shared, &views, &Disk);
        let mut store = lock(&shared);
        store.views = views;
        store.known = known;
        store.answers = answers;
        store.scanning = false;
        store.scanned = Some(Instant::now());
    });
}

/// Connects to the admin channel of each Empyrean in `views` the launcher did not start and is not
/// connected to, and lets go of those whose world is gone.
fn connect_channels(shared: &Shared, views: &[View], files: &dyn Files) {
    let wanted: Vec<(String, PathBuf, u32)> = {
        let store = lock(shared);
        let ours: HashSet<&str> = store.children.iter().map(|c| c.key.as_str()).collect();
        views
            .iter()
            .filter(|v| v.running && v.software == Some(Software::Empyrean))
            .filter(|v| !ours.contains(v.key.as_str()))
            .filter_map(|v| Some((v.key.clone(), v.folder.clone()?, v.pid?)))
            .filter(|(key, _, pid)| {
                !store
                    .channels
                    .get(key)
                    .is_some_and(|c| c.alive() && c.pid == *pid)
            })
            .collect()
    };
    let mut made = Vec::new();
    for (key, folder, pid) in wanted {
        if let Some(c) =
            local::admin_channel(&folder, pid, files).and_then(|c| connect_channel(&c, pid))
        {
            made.push((key, c));
        }
    }
    let mut store = lock(shared);
    let running: HashSet<String> = views
        .iter()
        .filter(|v| v.running)
        .map(|v| v.key.clone())
        .collect();
    store
        .channels
        .retain(|key, c| c.alive() && running.contains(key));
    store.channels.extend(made);
}

/// The page asks for a look as Host a world opens, and every few seconds while it is open.
#[tauri::command]
pub fn scan_local_worlds(shared: State<'_, Shared>, force: bool) {
    scan(shared.inner(), force);
}

fn reader(
    input: impl Read + Send + 'static,
    log: Arc<Mutex<VecDeque<String>>>,
    file: Option<Arc<Mutex<std::fs::File>>>,
) {
    std::thread::spawn(move || {
        let mut lines = std::io::BufReader::new(input);
        let mut line = String::new();
        use std::io::BufRead;
        while lines.read_line(&mut line).is_ok_and(|n| n > 0) {
            let shown = crate::host_config::redact(line.trim_end_matches(['\r', '\n']));
            if let Some(file) = &file {
                let _ = writeln!(
                    file.lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner),
                    "{shown}"
                );
            }
            let mut log = log
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if log.len() == 300 {
                log.pop_front();
            }
            log.push_back(shown);
            line.clear();
        }
    });
}

/// Start a remembered Empyrean world from its folder, as the launcher's own child.
#[tauri::command]
pub fn start_local_world(shared: State<'_, Shared>, key: String) -> Result<(), String> {
    let mut store = lock(&shared);
    let view = store
        .views
        .iter()
        .find(|v| v.key == key)
        .cloned()
        .ok_or("That world is no longer here.")?;
    if view.running || store.children.iter().any(|c| c.key == key) {
        return Err("This world is already running.".into());
    }
    if let Some(why) = view.cannot_start {
        return Err(why);
    }
    // A world a service manager runs is started there, when the launcher may.
    if let Some(manager) = view.manager.as_ref().filter(|m| m.service().is_some()) {
        let services = Arc::clone(&store.services);
        let manager = manager.clone();
        store.messages.remove(&key);
        store.scanned = None;
        drop(store);
        return services.start(&manager);
    }
    let known = store
        .known
        .iter()
        .find(|k| k.key == key)
        .cloned()
        .ok_or("The launcher does not know where this world is.")?;
    let folder = known
        .config
        .parent()
        .ok_or("Invalid configuration path")?
        .to_path_buf();
    // Its output is kept beside the launcher's own worlds' logs, never in the world's folder.
    let logs = store.data.join("local-logs");
    let _ = std::fs::create_dir_all(&logs);
    let name: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs.join(format!("{}.log", &name[name.len().saturating_sub(80)..])))
        .ok()
        .map(|f| Arc::new(Mutex::new(f)));
    let mut child = crate::components::command(&known.program)
        .arg("--config")
        .arg(&known.config)
        .current_dir(&folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start the world: {e}"))?;
    let log = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(out) = child.stdout.take() {
        reader(out, log.clone(), file.clone());
    }
    if let Some(err) = child.stderr.take() {
        reader(err, log.clone(), file);
    }
    let stdin = child.stdin.take();
    store.children.push(Started {
        key: key.clone(),
        child,
        stdin,
        log,
        stopping: false,
        started: now(),
    });
    store.messages.remove(&key);
    if let Some(v) = store.views.iter_mut().find(|v| v.key == key) {
        v.running = true;
        v.state = "starting".into();
    }
    store.scanned = None;
    Ok(())
}

/// Stop a world the launcher started: its console's `exit`, which saves first. Asked again while
/// it stops, with `now`, it is killed. A world the launcher did not start is never stopped here.
#[tauri::command]
pub fn stop_local_world(shared: State<'_, Shared>, key: String, now: bool) -> Result<(), String> {
    let mut store = lock(&shared);
    if !store.children.iter().any(|c| c.key == key) {
        return stop_elsewhere(&mut store, &key);
    }
    let Some(started) = store.children.iter_mut().find(|c| c.key == key) else {
        return Err("The launcher did not start this world, so it leaves it alone. Stop it where it was started.".into());
    };
    if started.stopping {
        if now {
            started
                .child
                .kill()
                .map_err(|e| format!("Could not stop the world: {e}"))?;
        }
        return Ok(());
    }
    started.stopping = true;
    if let Some(stdin) = started.stdin.as_mut() {
        writeln!(stdin, "exit")
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("Could not ask the world to stop: {e}"))?;
    }
    Ok(())
}

/// Stops a world the launcher did not start. A service the launcher may start and stop is stopped
/// through its service manager, which has the world save and stop, so a service set to start its
/// world again when it exits cannot undo the Stop. Any other world with an admin channel is asked
/// there (`exit`, which saves first).
fn stop_elsewhere(store: &mut Store, key: &str) -> Result<(), String> {
    let view = store
        .views
        .iter()
        .find(|v| v.key == key)
        .cloned()
        .ok_or("That world is no longer here.");
    if let Ok(View {
        manager: Some(manager),
        control: Some(Control {
            can_control: true, ..
        }),
        ..
    }) = &view
    {
        store.services.stop(manager)?;
        if let Some(c) = store.channels.get_mut(key) {
            c.stopping = true;
        }
        return Ok(());
    }
    if let Some(c) = store.channels.get_mut(key).filter(|c| c.alive()) {
        c.send("exit")
            .map_err(|e| format!("Could not ask the world to stop: {e}"))?;
        c.stopping = true;
        return Ok(());
    }
    match view?.control {
        Some(Control {
            stop_command: Some(command),
            ..
        }) => Err(format!("Stop it with: {command}")),
        _ => Err("The launcher did not start this world, so it leaves it alone. Stop it where it was started.".into()),
    }
}

/// Let the launcher start and stop the Windows service that runs a world: one administrator's
/// permission, asked with Windows' own prompt.
#[tauri::command]
pub async fn allow_service_control(shared: State<'_, Shared>, key: String) -> Result<(), String> {
    let (services, name) = {
        let store = lock(&shared);
        let view = store
            .views
            .iter()
            .find(|v| v.key == key)
            .ok_or("That world is no longer here.")?;
        let Some(Manager::WindowsService { name }) = view.manager.clone() else {
            return Err("Only a Windows service is set up this way.".into());
        };
        (Arc::clone(&store.services), name)
    };
    let shared = shared.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = services.grant(&name);
        lock(&shared).scanned = None;
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Pass a line to the console of a world the launcher started, or of one connected to through its
/// admin channel.
#[tauri::command]
pub fn local_command(shared: State<'_, Shared>, key: String, line: String) -> Result<(), String> {
    let line = line.trim().to_owned();
    if line.is_empty() || line.len() > 1000 || line.chars().any(char::is_control) {
        return Err("A command is one line of up to 1000 characters.".into());
    }
    let mut store = lock(&shared);
    if !store.children.iter().any(|c| c.key == key) {
        let c = store
            .channels
            .get(&key)
            .filter(|c| c.alive())
            .ok_or("This world takes no commands from here.")?;
        c.send(&line)
            .map_err(|e| format!("Could not pass the command on: {e}"))?;
        c.show(crate::host_runtime::shown_command(&line));
        return Ok(());
    }
    let started = store
        .children
        .iter_mut()
        .find(|c| c.key == key)
        .ok_or("Only a world the launcher started takes commands from here.")?;
    let stdin = started.stdin.as_mut().ok_or("The world is stopping.")?;
    writeln!(stdin, "{line}")
        .and_then(|()| stdin.flush())
        .map_err(|e| format!("Could not pass the command on: {e}"))?;
    started
        .log
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push_back(crate::host_runtime::shown_command(&line));
    Ok(())
}

/// Forget a remembered world that is not running.
#[tauri::command]
pub fn forget_local_world(shared: State<'_, Shared>, key: String) -> Result<(), String> {
    let mut store = lock(&shared);
    if store.views.iter().any(|v| v.key == key && v.running) {
        return Err("Stop the world first.".into());
    }
    store.known.retain(|k| k.key != key);
    store.views.retain(|v| v.key != key);
    let path = store.data.join(KNOWN_FILE);
    crate::host_runtime::write_json(&path, &store.known)
}

/// Show one of a world's places in the system's file browser: its `folder`, `config`, `data`,
/// `dats` or `log`.
#[tauri::command]
pub fn reveal_local_world(
    shared: State<'_, Shared>,
    key: String,
    place: String,
) -> Result<(), String> {
    let view = lock(&shared)
        .views
        .iter()
        .find(|v| v.key == key)
        .cloned()
        .ok_or("That world is no longer here.")?;
    let path = match place.as_str() {
        "folder" => view.folder,
        "config" => view.config,
        "data" => view.data,
        "dats" => view.dats,
        "log" => view.log,
        _ => None,
    }
    .ok_or("The launcher does not know where that is.")?;
    reveal(&path)
}

/// Show `path` in the system's file browser: a folder opened, a file selected in its folder.
pub fn reveal(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("{} is not there.", path.display()));
    }
    if path.is_dir() {
        return crate::components::open_folder(path);
    }
    let mut command = if cfg!(windows) {
        let mut c = std::process::Command::new("explorer.exe");
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        c.arg(arg);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(path);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(path));
        c
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open {}: {e}", path.display()))
}

/// Whether the launcher started any world that is still running.
pub fn any_started(shared: &Shared) -> bool {
    let mut store = lock(shared);
    store
        .children
        .retain_mut(|c| !matches!(c.child.try_wait(), Ok(Some(_))));
    !store.children.is_empty()
}

/// Ask every world the launcher started to stop.
pub fn stop_all(shared: &Shared) {
    let mut store = lock(shared);
    for c in &mut store.children {
        if !c.stopping {
            c.stopping = true;
            if let Some(stdin) = c.stdin.as_mut() {
                let _ = writeln!(stdin, "exit").and_then(|()| stdin.flush());
            }
        }
    }
}

// ----- the system ---------------------------------------------------------------------------------

/// Whether a program's file name could be a server's, before anything more is read of it.
fn could_be_server(name: &str) -> bool {
    let name = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    let stem = name.strip_suffix(".exe").unwrap_or(&name);
    matches!(stem, "empyrean-server" | "ace.server" | "dotnet") || stem.starts_with("gdlenhanced")
}

/// The system the launcher runs on.
pub struct OsSystem;

impl System for OsSystem {
    fn servers(&self) -> Vec<Running> {
        if cfg!(windows) {
            windows_servers()
        } else if cfg!(target_os = "macos") {
            mac_servers()
        } else {
            linux_servers()
        }
    }
}

#[cfg(not(windows))]
fn windows_servers() -> Vec<Running> {
    Vec::new()
}

#[cfg(windows)]
fn windows_servers() -> Vec<Running> {
    let listed = windows::processes();
    let parents: HashMap<u32, u32> = listed
        .iter()
        .map(|(pid, parent, _)| (*pid, *parent))
        .collect();
    let candidates: Vec<(u32, u32, String)> = listed
        .into_iter()
        .filter(|(_, _, name)| could_be_server(name))
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }
    let pids: HashSet<u32> = candidates.iter().map(|(pid, _, _)| *pid).collect();
    let ports = windows::udp_ports(&pids);
    let services = windows::services();
    candidates
        .into_iter()
        .map(|(pid, parent, name)| {
            let (process, started) = windows::describe(pid, Some(parent), &name);
            let manager = local::service_of(
                pid,
                &|p| parents.get(&p).copied(),
                &|p| services.get(&p).cloned(),
                3,
            )
            .map_or(Manager::Elsewhere, |name| Manager::WindowsService { name });
            Running {
                process,
                started,
                ports: ports.get(&pid).cloned().unwrap_or_default(),
                manager,
            }
        })
        .collect()
}

/// Linux: `/proc`.
fn linux_servers() -> Vec<Running> {
    let proc = Path::new("/proc");
    let Ok(entries) = std::fs::read_dir(proc) else {
        return Vec::new();
    };
    let mut sockets: HashMap<u64, (IpAddr, u16)> = HashMap::new();
    for table in ["net/udp", "net/udp6"] {
        if let Ok(text) = std::fs::read_to_string(proc.join(table)) {
            for (ip, port, inode) in local::proc_net_udp(&text) {
                sockets.insert(inode, (ip, port));
            }
        }
    }
    let boot = std::fs::read_to_string(proc.join("stat"))
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("btime ")?.trim().parse::<u64>().ok())
        });
    let mut out = Vec::new();
    for e in entries.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        let dir = e.path();
        let args: Vec<String> = std::fs::read(dir.join("cmdline"))
            .map(|b| {
                b.split(|c| *c == 0)
                    .filter(|a| !a.is_empty())
                    .map(|a| String::from_utf8_lossy(a).into_owned())
                    .collect()
            })
            .unwrap_or_default();
        let program = std::fs::read_link(dir.join("exe"))
            .ok()
            .or_else(|| args.first().map(PathBuf::from));
        let Some(program) = program else {
            continue;
        };
        if !could_be_server(&program.to_string_lossy()) {
            continue;
        }
        let stat = std::fs::read_to_string(dir.join("stat")).unwrap_or_default();
        // The fields after the program's name, which is in brackets and may hold spaces.
        let after: Vec<&str> = stat
            .rsplit_once(')')
            .map(|(_, rest)| rest.split_whitespace().collect())
            .unwrap_or_default();
        let parent = after.get(1).and_then(|p| p.parse().ok());
        let started = after
            .get(19)
            .and_then(|t| t.parse::<u64>().ok())
            .zip(boot)
            .map(|(ticks, boot)| boot + ticks / 100);
        let ports = std::fs::read_dir(dir.join("fd"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|fd| std::fs::read_link(fd.path()).ok())
            .filter_map(|link| local::socket_inode(&link.to_string_lossy()))
            .filter_map(|inode| sockets.get(&inode).copied())
            .collect();
        let manager = std::fs::read_to_string(dir.join("cgroup"))
            .ok()
            .and_then(|c| local::systemd_unit(&c))
            .map_or(Manager::Elsewhere, |(unit, user)| Manager::Systemd {
                unit,
                user,
            });
        out.push(Running {
            process: Process {
                pid,
                parent,
                program,
                args,
                cwd: std::fs::read_link(dir.join("cwd")).ok(),
            },
            started,
            ports,
            manager,
        });
    }
    out
}

/// A Mac: `ps` and `lsof`.
fn mac_servers() -> Vec<Running> {
    let run = |program: &str, args: &[&str]| -> String {
        crate::components::command(Path::new(program))
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    for line in run("/bin/ps", &["-axww", "-o", "pid=,ppid=,comm="]).lines() {
        let mut parts = line.split_whitespace();
        let (Some(pid), Some(parent)) = (
            parts.next().and_then(|p| p.parse::<u32>().ok()),
            parts.next().and_then(|p| p.parse::<u32>().ok()),
        ) else {
            continue;
        };
        let program = parts.collect::<Vec<_>>().join(" ");
        if !could_be_server(&program) {
            continue;
        }
        let pid_text = pid.to_string();
        let args: Vec<String> = run("/bin/ps", &["-ww", "-o", "args=", "-p", &pid_text])
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let cwd = run(
            "/usr/sbin/lsof",
            &["-a", "-d", "cwd", "-p", &pid_text, "-Fn"],
        )
        .lines()
        .find_map(|l| l.strip_prefix('n').map(PathBuf::from));
        let ports = local::lsof_ports(&run(
            "/usr/sbin/lsof",
            &["-nP", "-a", "-iUDP", "-p", &pid_text, "-Fn"],
        ))
        .into_iter()
        .map(|p| (IpAddr::from([0, 0, 0, 0]), p))
        .collect();
        out.push(Running {
            process: Process {
                pid,
                parent: Some(parent),
                program: PathBuf::from(program),
                args,
                cwd,
            },
            started: None,
            ports,
            manager: if parent == 1 {
                Manager::Launchd
            } else {
                Manager::Elsewhere
            },
        });
    }
    out
}

#[cfg(windows)]
mod windows {
    //! The process list, a process's command line, the UDP table and the services, from Windows.

    use super::{IpAddr, PathBuf, Process};
    use std::collections::{HashMap, HashSet};
    use windows_sys::Wdk::System::Threading::{
        NtQueryInformationProcess, ProcessCommandLineInformation,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER, ERROR_MORE_DATA, FILETIME,
        INVALID_HANDLE_VALUE, NO_ERROR,
    };
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedUdpTable, UDP_TABLE_OWNER_PID,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, EnumServicesStatusExW, OpenSCManagerW, ENUM_SERVICE_STATUS_PROCESSW,
        SC_ENUM_PROCESS_INFO, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_ACTIVE, SERVICE_WIN32,
    };
    use windows_sys::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

    fn wide(s: &[u16]) -> String {
        let end = s.iter().position(|c| *c == 0).unwrap_or(s.len());
        String::from_utf16_lossy(&s[..end])
    }

    /// A null-terminated wide string at `p`.
    ///
    /// # Safety
    /// `p` is null or points at a null-terminated string.
    unsafe fn wide_at(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }

    /// Every process: its id, its starter's and its program's file name.
    pub fn processes() -> Vec<(u32, u32, String)> {
        let mut out = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snapshot, &mut entry) != 0;
            while more {
                out.push((
                    entry.th32ProcessID,
                    entry.th32ParentProcessID,
                    wide(&entry.szExeFile),
                ));
                more = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        out
    }

    /// One process, as far as Windows lets the launcher read it: its program's full path, its
    /// command line and when it started. A process another account runs may say only its name.
    pub fn describe(pid: u32, parent: Option<u32>, name: &str) -> (Process, Option<u64>) {
        let mut process = Process {
            pid,
            parent,
            program: PathBuf::from(name),
            args: Vec::new(),
            cwd: None,
        };
        let mut started = None;
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return (process, None);
            }
            let mut buf = vec![0u16; 32768];
            let mut len = buf.len() as u32;
            if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
                != 0
            {
                process.program = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
            }
            // The command line: a UNICODE_STRING at the start of the buffer, its text after it.
            let mut needed = 0u32;
            let _ = NtQueryInformationProcess(
                handle,
                ProcessCommandLineInformation,
                std::ptr::null_mut(),
                0,
                &mut needed,
            );
            if needed > 0 && needed < 1 << 20 {
                let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
                if NtQueryInformationProcess(
                    handle,
                    ProcessCommandLineInformation,
                    buf.as_mut_ptr().cast(),
                    needed,
                    &mut needed,
                ) >= 0
                {
                    let text = &*(buf
                        .as_ptr()
                        .cast::<windows_sys::Win32::Foundation::UNICODE_STRING>());
                    if !text.Buffer.is_null() {
                        let chars =
                            std::slice::from_raw_parts(text.Buffer, usize::from(text.Length) / 2);
                        let mut line: Vec<u16> = chars.to_vec();
                        line.push(0);
                        let mut count = 0;
                        let argv = CommandLineToArgvW(line.as_ptr(), &mut count);
                        if !argv.is_null() {
                            for i in 0..usize::try_from(count).unwrap_or(0) {
                                process.args.push(wide_at(*argv.add(i)));
                            }
                            LocalFree(argv.cast());
                        }
                    }
                }
            }
            let (mut created, mut exited, mut kernel, mut user) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            if GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0 {
                let ticks =
                    (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
                started = (ticks / 10_000_000).checked_sub(11_644_473_600);
            }
            CloseHandle(handle);
        }
        (process, started)
    }

    /// The IPv4 UDP ports each of `pids` listens on.
    pub fn udp_ports(pids: &HashSet<u32>) -> HashMap<u32, Vec<(IpAddr, u16)>> {
        let mut out: HashMap<u32, Vec<(IpAddr, u16)>> = HashMap::new();
        unsafe {
            let mut size = 0u32;
            let mut buf: Vec<u32> = Vec::new();
            for _ in 0..4 {
                let code = GetExtendedUdpTable(
                    if buf.is_empty() {
                        std::ptr::null_mut()
                    } else {
                        buf.as_mut_ptr().cast()
                    },
                    &mut size,
                    0,
                    u32::from(AF_INET),
                    UDP_TABLE_OWNER_PID,
                    0,
                );
                if code == NO_ERROR && !buf.is_empty() {
                    break;
                }
                if code != ERROR_INSUFFICIENT_BUFFER && code != NO_ERROR {
                    return out;
                }
                buf = vec![0u32; (size as usize).div_ceil(4) + 4];
                size = (buf.len() * 4) as u32;
            }
            if buf.is_empty() {
                return out;
            }
            for (pid, ip, port) in super::udp_rows(&buf) {
                if pids.contains(&pid) {
                    out.entry(pid).or_default().push((ip, port));
                }
            }
        }
        out
    }

    /// The services running, by their process.
    pub fn services() -> HashMap<u32, String> {
        let mut out = HashMap::new();
        unsafe {
            let manager = OpenSCManagerW(
                std::ptr::null(),
                std::ptr::null(),
                SC_MANAGER_ENUMERATE_SERVICE,
            );
            if manager.is_null() {
                return out;
            }
            let mut buf: Vec<u64> = vec![0; 8192];
            let mut resume = 0u32;
            loop {
                let mut needed = 0u32;
                let mut count = 0u32;
                let ok = EnumServicesStatusExW(
                    manager,
                    SC_ENUM_PROCESS_INFO,
                    SERVICE_WIN32,
                    SERVICE_ACTIVE,
                    buf.as_mut_ptr().cast(),
                    (buf.len() * 8) as u32,
                    &mut needed,
                    &mut count,
                    &mut resume,
                    std::ptr::null(),
                ) != 0;
                let more = !ok && windows_sys::Win32::Foundation::GetLastError() == ERROR_MORE_DATA;
                let entries = std::slice::from_raw_parts(
                    buf.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>(),
                    count as usize,
                );
                for e in entries {
                    let pid = e.ServiceStatusProcess.dwProcessId;
                    if pid != 0 {
                        out.insert(pid, wide_at(e.lpServiceName));
                    }
                }
                if !more {
                    break;
                }
                if count == 0 {
                    buf = vec![0; (needed as usize).div_ceil(8) + 1];
                }
            }
            CloseServiceHandle(manager);
        }
        out
    }
}

/// The rows of a Windows UDP table that names each socket's process
/// (`MIB_UDPTABLE_OWNER_PID`, as `u32` words): each process, its local address and its port.
#[cfg_attr(not(windows), allow(dead_code))]
fn udp_rows(table: &[u32]) -> Vec<(u32, IpAddr, u16)> {
    let count = table.first().copied().unwrap_or(0) as usize;
    table
        .get(1..)
        .unwrap_or_default()
        .as_chunks::<3>()
        .0
        .iter()
        .take(count)
        .map(|row| {
            let ip = IpAddr::from(row[0].to_ne_bytes());
            let port = row[1].to_ne_bytes();
            (row[2], ip, u16::from_be_bytes([port[0], port[1]]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fake(Vec<Running>);
    impl System for Fake {
        fn servers(&self) -> Vec<Running> {
            self.0.clone()
        }
    }

    #[derive(Default)]
    struct FakeFiles(BTreeMap<PathBuf, String>);
    impl Files for FakeFiles {
        fn read(&self, path: &Path) -> Option<String> {
            self.0.get(path).cloned()
        }
        fn is_file(&self, path: &Path) -> bool {
            self.0.contains_key(path)
        }
        fn list(&self, _dir: &Path) -> Vec<(PathBuf, u64)> {
            Vec::new()
        }
    }

    fn running(pid: u32, program: &str, args: &[&str], manager: Manager) -> Running {
        Running {
            process: Process {
                pid,
                parent: None,
                program: program.into(),
                args: std::iter::once(program)
                    .chain(args.iter().copied())
                    .map(str::to_owned)
                    .collect(),
                cwd: None,
            },
            started: Some(1_700_000_000),
            ports: vec![
                (IpAddr::from([0, 0, 0, 0]), 9000),
                (IpAddr::from([0, 0, 0, 0]), 9001),
            ],
            manager,
        }
    }

    /// The servers on a computer, as a fake process list gives them: a service, one started by
    /// hand, one of the launcher's own (left out), and a program that is no server.
    #[test]
    fn running_servers_are_found_with_their_layout_answer_and_who_controls_them() {
        let files = FakeFiles(BTreeMap::from([
            (
                PathBuf::from("/srv/emp/empyrean.toml"),
                "[server]\nworld_name='Service World'\n".into(),
            ),
            (PathBuf::from("/srv/emp/empyrean-server"), String::new()),
            (
                PathBuf::from("/ace/Config.js"),
                "{\"Server\":{\"WorldName\":\"Ace World\",\"Network\":{\"Port\":9050}}}".into(),
            ),
        ]));
        let system = Fake(vec![
            running(
                10,
                "/srv/emp/empyrean-server",
                &["--config", "/srv/emp/empyrean.toml"],
                Manager::Systemd {
                    unit: "empyrean.service".into(),
                    user: false,
                },
            ),
            running(11, "/ace/ACE.Server", &[], Manager::Elsewhere),
            running(
                12,
                "/opt/x/empyrean-server",
                &["--config", "/data/launcher/hosts/host-1/empyrean.toml"],
                Manager::Elsewhere,
            ),
            running(13, "/usr/bin/vim", &[], Manager::Elsewhere),
        ]);
        let mut known = Vec::new();
        let mut asked = Vec::new();
        let views = assemble(
            &system,
            &files,
            Path::new("/data/launcher/hosts"),
            &mut known,
            &crate::services::tests::FakeServices::default(),
            &mut |key, software, ip, port| {
                asked.push((key.to_owned(), software, ip, port));
                Answer {
                    state: "online".into(),
                    name: (software == Software::Empyrean).then(|| "Answered Name".into()),
                    version: Some("0.3.3".into()),
                    players: Some(2),
                }
            },
        );
        assert_eq!(views.len(), 2, "{views:#?}");
        let emp = &views[0];
        assert_eq!(
            emp.name, "Answered Name",
            "what it answers wins over its file"
        );
        assert_eq!(emp.software, Some(Software::Empyrean));
        assert_eq!(emp.port, Some(9000));
        assert_eq!(emp.players, Some(2));
        assert_eq!(
            emp.manager,
            Some(Manager::Systemd {
                unit: "empyrean.service".into(),
                user: false
            })
        );
        assert_eq!(
            emp.config.as_deref(),
            Some(Path::new("/srv/emp/empyrean.toml"))
        );
        let ace = &views[1];
        assert_eq!(ace.name, "Ace World");
        // ACE listens on 9000 and 9001 though its file says 9050: the ports it holds win.
        assert_eq!(ace.port, Some(9000));
        assert_eq!(asked.len(), 2);
        assert_eq!(asked[0].2, IpAddr::from([127, 0, 0, 1]));
        // The Empyrean running as a service is remembered, and as a service.
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].service.as_deref(), Some("empyrean.service"));
    }

    /// Once it stops, a remembered world is shown stopped, and may be started from its folder
    /// unless a service ran it or its console is off.
    #[test]
    fn a_remembered_world_that_stopped_can_be_started_only_from_a_folder_the_launcher_may_control()
    {
        let mut files = FakeFiles(BTreeMap::from([
            (
                PathBuf::from("/w/a/empyrean.toml"),
                "[server]\nworld_name='A'\n".into(),
            ),
            (PathBuf::from("/w/a/empyrean-server"), String::new()),
            (
                PathBuf::from("/w/b/empyrean.toml"),
                "[server]\ninteractive_console=false\n".into(),
            ),
            (PathBuf::from("/w/b/empyrean-server"), String::new()),
            (PathBuf::from("/w/c/empyrean.toml"), String::new()),
            (PathBuf::from("/w/c/empyrean-server"), String::new()),
        ]));
        let k = |name: &str, service: Option<&str>| Known {
            key: format!("empyrean:/w/{name}/empyrean.toml"),
            program: format!("/w/{name}/empyrean-server").into(),
            config: format!("/w/{name}/empyrean.toml").into(),
            name: None,
            service: service.map(str::to_owned),
            manager: None,
        };
        let mut known = vec![k("a", None), k("b", None), k("c", Some("empyrean"))];
        let views = assemble(
            &Fake(Vec::new()),
            &files,
            Path::new("/hosts"),
            &mut known,
            &crate::services::tests::FakeServices::default(),
            &mut |_, _, _, _| unreachable!("a stopped world is not asked"),
        );
        assert_eq!(views.len(), 3);
        assert!(views.iter().all(|v| !v.running));
        assert_eq!(views[0].name, "A");
        assert_eq!(views[0].cannot_start, None);
        assert!(views[1]
            .cannot_start
            .as_deref()
            .unwrap()
            .contains("console is off"));
        assert!(views[2]
            .cannot_start
            .as_deref()
            .unwrap()
            .contains("service empyrean"));
        files.0.remove(Path::new("/w/a/empyrean-server"));
        let views = assemble(
            &Fake(Vec::new()),
            &files,
            Path::new("/hosts"),
            &mut known,
            &crate::services::tests::FakeServices::default(),
            &mut |_, _, _, _| Answer::default(),
        );
        assert!(views[0]
            .cannot_start
            .as_deref()
            .unwrap()
            .contains("no longer there"));
    }

    #[test]
    fn only_a_servers_program_names_are_looked_at_closer() {
        for name in [
            "empyrean-server.exe",
            "/opt/e/empyrean-server",
            "ACE.Server.exe",
            "dotnet",
            "GDLEnhanced - Debug.exe",
        ] {
            assert!(could_be_server(name), "{name}");
        }
        for name in [
            "explorer.exe",
            "empyrean-import",
            "dereth.exe",
            "svchost.exe",
        ] {
            assert!(!could_be_server(name), "{name}");
        }
    }

    #[test]
    fn a_windows_udp_table_gives_each_sockets_process_address_and_port() {
        // Two rows: 0.0.0.0:9000 for process 40, 127.0.0.1:9001 for process 41. The address and
        // the port are in network order, as Windows writes them.
        let port = |p: u16| u32::from_ne_bytes([(p >> 8) as u8, p as u8, 0, 0]);
        let table = [
            2,
            0,
            port(9000),
            40,
            u32::from_ne_bytes([127, 0, 0, 1]),
            port(9001),
            41,
            99,
        ];
        assert_eq!(
            udp_rows(&table),
            [
                (40, IpAddr::from([0, 0, 0, 0]), 9000),
                (41, IpAddr::from([127, 0, 0, 1]), 9001),
            ]
        );
    }

    /// The stand-in server [`a_server_started_here_is_found_by_its_process_ports_and_answer`]
    /// starts: a copy of this test program, named as Empyrean's, listening on two neighbouring
    /// UDP ports and answering the status ping, until told to stop.
    #[test]
    #[ignore = "a stand-in server, run only as the child another test starts"]
    fn stand_in_server() {
        use dereth_launch::probe::status_ping as sp;
        let Some(dir) = std::env::var_os("DERETH_STAND_IN_DIR").map(PathBuf::from) else {
            return;
        };
        let (game, _next) = loop {
            let a = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            let port = a.local_addr().unwrap().port();
            if let Ok(b) = std::net::UdpSocket::bind(("127.0.0.1", port + 1)) {
                break (a, b);
            }
        };
        game.set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let port = game.local_addr().unwrap().port();
        std::fs::write(dir.join("port.tmp"), port.to_string()).unwrap();
        std::fs::rename(dir.join("port.tmp"), dir.join("port")).unwrap();
        let token = sp::Token {
            window: 1,
            mac: [7; 16],
        };
        let mut buf = [0u8; 1500];
        while !dir.join("stop").exists() {
            let Ok((n, from)) = game.recv_from(&mut buf) else {
                continue;
            };
            let reply = match sp::parse_request(&buf[..n]) {
                Some(sp::Request::Hello) => sp::token_reply(token),
                Some(sp::Request::Ask(t)) if t == token => sp::StatusReply {
                    format_version: 1,
                    state: sp::WorldState::Open,
                    players: 3,
                    era: "eor".into(),
                    era_table_version: 1,
                    era_features: Vec::new(),
                    software: "Empyrean".into(),
                    software_version: "9.9.9".into(),
                    world_name: "Stand-in".into(),
                }
                .encode(),
                _ => continue,
            };
            let _ = game.send_to(&reply, from);
        }
    }

    /// The system, as far as one process this test started.
    #[cfg(windows)]
    struct OneProcess(u32);

    #[cfg(windows)]
    impl System for OneProcess {
        fn servers(&self) -> Vec<Running> {
            let (process, started) = windows::describe(self.0, None, "empyrean-server.exe");
            let ports = windows::udp_ports(&HashSet::from([self.0]));
            vec![Running {
                process,
                started,
                ports: ports.get(&self.0).cloned().unwrap_or_default(),
                manager: Manager::Elsewhere,
            }]
        }
    }

    /// A stand-in server this test starts (a copy of the test program, named as Empyrean's) is
    /// read as the system shows it: its program, its command line, its start, its two ports, its
    /// configuration beside it, and what it answers. Only that one process is looked at.
    #[cfg(windows)]
    #[test]
    fn a_server_started_here_is_found_by_its_process_ports_and_answer() {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("empyrean-server.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &program).unwrap();
        std::fs::write(
            dir.path().join("empyrean.toml"),
            "[server]
world_name = 'From the file'
",
        )
        .unwrap();
        let mut child = std::process::Command::new(&program)
            .args([
                "--exact",
                "local::tests::stand_in_server",
                "--ignored",
                "--quiet",
            ])
            .env("DERETH_STAND_IN_DIR", dir.path())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        // Wait for the stand-in to say its port; the limit only guards against a hang.
        let guard = Instant::now();
        let port_file = dir.path().join("port");
        while !port_file.exists() {
            assert!(
                guard.elapsed() < Duration::from_secs(120),
                "the stand-in never started"
            );
            assert!(child.try_wait().unwrap().is_none(), "the stand-in exited");
            std::thread::sleep(Duration::from_millis(20));
        }
        let port: u16 = std::fs::read_to_string(&port_file)
            .unwrap()
            .parse()
            .unwrap();
        let found = OneProcess(child.id()).servers();
        let mut known = Vec::new();
        let views = assemble(
            &OneProcess(child.id()),
            &Disk,
            Path::new("Z:/nowhere"),
            &mut known,
            &crate::services::tests::FakeServices::default(),
            &mut |_, software, ip, port| ask_world(software, ip, port),
        );
        std::fs::write(dir.path().join("stop"), b"").unwrap();
        let stopped = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if stopped.elapsed() > Duration::from_secs(120) {
                let _ = child.kill();
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let running = &found[0];
        assert_eq!(
            running.process.program.canonicalize().unwrap(),
            program.canonicalize().unwrap()
        );
        assert!(
            running
                .process
                .args
                .iter()
                .any(|a| a == "local::tests::stand_in_server"),
            "{:?}",
            running.process.args
        );
        assert!(running.started.is_some_and(|s| s + 600 > now()));
        let ports: Vec<u16> = running.ports.iter().map(|(_, p)| *p).collect();
        assert!(
            ports.contains(&port) && ports.contains(&(port + 1)),
            "{ports:?}"
        );
        assert_eq!(views.len(), 1);
        let v = &views[0];
        assert_eq!(v.software, Some(Software::Empyrean));
        assert_eq!(
            v.port,
            Some(port),
            "the pair it holds, its file naming none"
        );
        assert_eq!(
            v.config
                .as_deref()
                .map(Path::canonicalize)
                .transpose()
                .unwrap(),
            Some(dir.path().join("empyrean.toml").canonicalize().unwrap())
        );
        assert_eq!(v.name, "Stand-in");
        assert_eq!(v.state, "online");
        assert_eq!(v.players, Some(3));
        assert_eq!(v.version.as_deref(), Some("9.9.9"));
        assert_eq!(
            known.len(),
            1,
            "an Empyrean run from a folder is remembered"
        );
    }

    #[test]
    fn a_log_file_is_read_from_its_end_with_credentials_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.log");
        let mut text = String::new();
        for i in 0..300 {
            text.push_str(&format!("line {i}\n"));
        }
        text.push_str("login password=abc\n");
        std::fs::write(&path, text).unwrap();
        let lines = tail_of(&path);
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines.last().unwrap(), "[credential details hidden]");
        assert_eq!(lines[0], "line 101");
    }

    /// A stand-in admin channel on this computer: it hands back the first line it is sent (the
    /// secret), says hello, sends `lines`, and then hands back each command it is sent.
    fn stand_in_channel(
        lines: Vec<&'static str>,
    ) -> (std::net::SocketAddr, std::sync::mpsc::Receiver<String>) {
        use std::io::BufRead;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            let _ = tx.send(format!("secret {}", first.trim()));
            let mut w = stream;
            writeln!(
                w,
                r#"{{"hello":"empyrean","version":"9.9.9","world":"Stand-in"}}"#
            )
            .unwrap();
            for l in lines {
                writeln!(w, "{}", serde_json::json!({ "line": l })).unwrap();
            }
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let _ = tx.send(line);
            }
        });
        (address, rx)
    }

    /// The launcher connects to a world's admin channel with its secret, shows the world's lines
    /// (credentials hidden), and passes the page's commands on.
    #[test]
    fn an_admin_channel_brings_the_worlds_log_and_takes_its_commands() {
        let secret = "ab".repeat(32);
        let (address, said) =
            stand_in_channel(vec!["World ready.", "login password=hunter2", "Saved."]);
        let channel = connect_channel(
            &local::AdminChannel {
                address,
                secret: secret.clone(),
                pid: 7,
            },
            7,
        )
        .expect("connected");
        assert_eq!(said.recv().unwrap(), format!("secret {secret}"));
        // The lines arrive on the connection's own thread; the guard only stops a hang.
        let started = Instant::now();
        while channel.lines.lock().unwrap().len() < 3 {
            assert!(
                started.elapsed() < Duration::from_secs(120),
                "no lines came"
            );
            std::thread::yield_now();
        }
        assert_eq!(
            channel
                .lines
                .lock()
                .unwrap()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            ["World ready.", "[credential details hidden]", "Saved."]
        );
        channel.send("save-world").unwrap();
        assert_eq!(said.recv().unwrap(), "save-world");
        assert!(channel.alive());
    }

    /// A world that a service manager runs, and that the launcher may start, is not refused a
    /// start; one it may not says how it is started instead.
    #[test]
    fn a_service_world_is_started_where_the_launcher_may_and_otherwise_says_how() {
        let files = FakeFiles(BTreeMap::from([
            (PathBuf::from("/srv/emp/empyrean-server"), String::new()),
            (
                PathBuf::from("/srv/emp/empyrean.toml"),
                "[server]\nworld_name = 'Svc'\n".to_owned(),
            ),
        ]));
        let service = Manager::WindowsService {
            name: "EmpyreanSvc".into(),
        };
        let mut known = vec![Known {
            key: "empyrean:/srv/emp/empyrean.toml".into(),
            program: "/srv/emp/empyrean-server".into(),
            config: "/srv/emp/empyrean.toml".into(),
            name: None,
            service: Some("EmpyreanSvc".into()),
            manager: Some(service),
        }];
        let fake = crate::services::tests::FakeServices::default();
        let views = assemble(
            &Fake(Vec::new()),
            &files,
            Path::new("/hosts"),
            &mut known,
            &fake,
            &mut |_, _, _, _| Answer::default(),
        );
        assert_eq!(
            views[0].cannot_start.as_deref(),
            Some("It runs as the service EmpyreanSvc. Start it with: sc start EmpyreanSvc")
        );
        assert!(views[0].control.as_ref().is_some_and(|c| c.can_grant));
        fake.grant("EmpyreanSvc").unwrap();
        let views = assemble(
            &Fake(Vec::new()),
            &files,
            Path::new("/hosts"),
            &mut known,
            &fake,
            &mut |_, _, _, _| Answer::default(),
        );
        assert_eq!(views[0].cannot_start, None, "the launcher may start it now");
        assert!(views[0].control.as_ref().is_some_and(|c| c.can_control));
    }

    /// Once the person has let the launcher start and stop a world's service, Stop goes to the
    /// service manager and nothing goes over the admin channel, so a service that starts its world
    /// again when it exits cannot undo it. Without that, Stop is `exit` over the channel.
    #[test]
    fn a_service_the_launcher_may_control_is_stopped_by_its_manager_not_over_the_channel() {
        let fake = Arc::new(crate::services::tests::FakeServices::default());
        let data = tempfile::tempdir().unwrap();
        let shared = with_services(data.path().to_path_buf(), fake.clone());
        let manager = Manager::WindowsService {
            name: "EmpyreanSvc".into(),
        };
        let key = "empyrean:/srv/emp/empyrean.toml";
        for granted in [false, true] {
            if granted {
                fake.grant("EmpyreanSvc").unwrap();
            }
            let (address, said) = stand_in_channel(Vec::new());
            let channel = connect_channel(
                &local::AdminChannel {
                    address,
                    secret: "ab".repeat(32),
                    pid: 7,
                },
                7,
            )
            .expect("connected");
            assert!(said.recv().unwrap().starts_with("secret "));
            {
                let mut store = lock(&shared);
                store.views = vec![View {
                    key: key.into(),
                    running: true,
                    pid: Some(7),
                    manager: Some(manager.clone()),
                    control: Some(crate::services::control(&manager, &*fake)),
                    ..View::default()
                }];
                store.channels.insert(key.into(), channel);
                stop_elsewhere(&mut store, key).unwrap();
                // Whatever the channel was sent before this line has arrived once it has.
                store.channels[key].send("after the stop").unwrap();
            }
            let sent: Vec<String> = std::iter::from_fn(|| said.recv().ok())
                .take_while(|l| l != "after the stop")
                .collect();
            let asked = fake.asked.lock().unwrap().clone();
            if granted {
                assert!(sent.is_empty(), "{sent:?}");
                assert_eq!(
                    asked,
                    [
                        "grant EmpyreanSvc",
                        "stop WindowsService { name: \"EmpyreanSvc\" }"
                    ]
                );
            } else {
                assert_eq!(sent, ["exit"]);
                assert!(asked.is_empty(), "{asked:?}");
            }
            assert!(views(&shared)[0].stopping, "the page says it is stopping");
        }
    }
}
