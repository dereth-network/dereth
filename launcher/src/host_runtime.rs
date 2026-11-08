//! A host supervisor used by the launcher and native service managers.
//!
//! The supervisor owns the server's console: it writes the server's output to the log, and passes
//! it the commands the launcher queues in the world's `console` folder ([`queue_command`]), one
//! file a line, so a world running in the background takes them as one the launcher started does.
//! It stops the server through that console, which saves everyone in the world. A Yalain world
//! also stops by itself, saving, once its console's input ends ([`crate::host_yalain::update_config`]): a
//! supervisor that dies without stopping it takes its world down cleanly rather than leaving it
//! running with nobody holding its console.

use crate::host_config::Settings;
use dereth_launch::world::Game;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Definition {
    pub id: String,
    pub version: String,
    pub program: PathBuf,
    pub wrapper: PathBuf,
    /// The server runs this checkout's build in place (a development build's server), so it is
    /// never updated from a release and never runs in the background.
    #[serde(default)]
    pub checkout: bool,
    /// The game the world is of: Empyrean's (Asheron's Call), or Yalain's (Asheron's Call 2,
    /// [`crate::host_yalain`]).
    #[serde(default)]
    pub game: Game,
}

/// The configuration file a world of `game` keeps its settings in, in its folder: Empyrean's
/// `empyrean.toml`, or Yalain's `yalain.toml`, which takes its form.
#[must_use]
pub fn config_file(game: Game) -> &'static str {
    match game {
        Game::Dereth => "empyrean.toml",
        Game::Auberean => crate::host_yalain::CONFIG,
    }
}

/// A world's settings, read from its configuration file ([`config_file`]).
///
/// # Errors
/// When they are not there or do not read.
pub fn read_settings(dir: &Path, game: Game) -> Result<Settings, String> {
    match game {
        Game::Dereth => Settings::from_config(
            &std::fs::read_to_string(dir.join(config_file(game))).map_err(|e| e.to_string())?,
        ),
        Game::Auberean => crate::host_yalain::read(dir),
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Status {
    pub phase: String,
    pub players: Option<u64>,
    pub pid: Option<u32>,
    pub message: Option<String>,
    /// When the server was started (seconds since 1970), while it runs.
    #[serde(default)]
    pub started: Option<u64>,
}
pub fn read_definition(dir: &Path) -> Result<Definition, String> {
    let definition: Definition =
        serde_json::from_slice(&std::fs::read(dir.join("host.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if definition.id.is_empty()
        || !definition
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("Invalid world identity.".into());
    }
    if !definition.program.is_absolute() || !definition.wrapper.is_absolute() {
        return Err("A world's programs need absolute paths.".into());
    }
    Ok(definition)
}
/// The folder in a world's folder its own copy of its server lives in: one path whatever release
/// it runs, so what names the program, a firewall rule among them, keeps naming the one that runs.
pub const SERVER_DIR: &str = "server";

/// Where a Yalain world keeps the release its last update replaced, for Undo.
pub const PREVIOUS_SERVER_DIR: &str = "server-previous";

/// Move a world made when each release had a folder of its own (`runtime/<version>`) into its one
/// server folder ([`SERVER_DIR`]): once, with the world stopped, before it starts or updates. A
/// world on this checkout's build stays where it is, and so does an Empyrean world whose updater
/// keeps a record of an update in its folder: the record names its files where they are. The
/// definition, as it is now.
///
/// # Errors
/// No definition, or a folder that will not move.
pub fn settle_server(dir: &Path) -> Result<Definition, String> {
    let mut definition = read_definition(dir)?;
    let server = dir.join(SERVER_DIR);
    let Some(folder) = definition.program.parent().map(Path::to_path_buf) else {
        return Ok(definition);
    };
    if definition.checkout || folder == server || !folder.starts_with(dir.join("runtime")) {
        return Ok(definition);
    }
    if definition.game == Game::Dereth && folder.join(".empyrean-update").exists() {
        return Ok(definition);
    }
    let name = definition
        .program
        .file_name()
        .ok_or("Invalid server filename")?
        .to_owned();
    // A folder the definition doesn't name yet is a move cut short: the copy that runs is the one
    // the definition names.
    if server.exists() {
        std::fs::remove_dir_all(&server).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&folder, &server).map_err(|e| e.to_string())?;
    definition.program = server.join(name);
    write_json(&dir.join("host.json"), &definition)?;
    Ok(definition)
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let temp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())
}
pub fn request_stop(dir: &Path) -> Result<(), String> {
    std::fs::write(dir.join("stop.request"), b"stop").map_err(|e| e.to_string())
}
pub fn active(dir: &Path) -> bool {
    let Ok(file) = std::fs::OpenOptions::new()
        .write(true)
        .open(dir.join("run.lock"))
    else {
        return false;
    };
    file.try_lock().is_err()
}
pub fn status(dir: &Path) -> Status {
    let mut status = std::fs::read(dir.join("status.json"))
        .ok()
        .and_then(|s| serde_json::from_slice::<Status>(&s).ok())
        .unwrap_or_default();
    if !active(dir) {
        if matches!(status.phase.as_str(), "starting" | "running" | "stopping") {
            status.message = Some(
                "The world stopped unexpectedly. Check its log before starting it again.".into(),
            );
            status.phase = "failed".into();
        } else if status.phase.is_empty() {
            status.phase = "stopped".into();
        }
        status.pid = None;
        status.started = None;
    }
    status
}

/// The folder a world's queued console commands wait in.
const CONSOLE_DIR: &str = "console";

/// The longest console line the launcher passes on.
const COMMAND_LIMIT: usize = 1000;

/// A console line as it may be written to the log: an account's password is hidden.
#[must_use]
pub fn shown_command(line: &str) -> String {
    let words: Vec<&str> = line.split_whitespace().collect();
    let secret = match words
        .first()
        .map(|w| w.trim_start_matches(['@', '/']).to_ascii_lowercase())
    {
        Some(c) if c == "accountcreate" || c == "set-accountpassword" => Some(2),
        _ => None,
    };
    let shown: Vec<&str> = words
        .iter()
        .enumerate()
        .map(|(i, w)| if Some(i) == secret { "***" } else { w })
        .collect();
    format!("> {}", shown.join(" "))
}

/// Queue `line` for a running world's console. The supervisor passes it to the server within a
/// moment, in the order queued.
///
/// # Errors
/// A world that is not running, a line that is empty, too long or holds a line break, or a
/// queue that cannot be written.
pub fn queue_command(dir: &Path, line: &str) -> Result<(), String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("Type a command first.".into());
    }
    if line.len() > COMMAND_LIMIT || line.chars().any(char::is_control) {
        return Err("A command is one line of up to 1000 characters.".into());
    }
    if !active(dir) {
        return Err("Start the world first.".into());
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let queue = dir.join(CONSOLE_DIR);
    std::fs::create_dir_all(&queue).map_err(|e| e.to_string())?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let name = format!("{at:032}-{:08}", NEXT.fetch_add(1, Ordering::SeqCst));
    let temp = queue.join(format!("{name}.tmp"));
    std::fs::write(&temp, line).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, queue.join(format!("{name}.cmd"))).map_err(|e| e.to_string())
}

/// The queued commands, oldest first, taken out of the queue.
fn take_commands(dir: &Path) -> Vec<String> {
    let queue = dir.join(CONSOLE_DIR);
    let Ok(entries) = std::fs::read_dir(&queue) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "cmd"))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|f| {
            let line = std::fs::read_to_string(&f).ok();
            let _ = std::fs::remove_file(&f);
            line
        })
        .collect()
}
/// The server's log, in its folder: everything it printed, credential lines hidden. Past
/// [`LOG_KEPT_BYTES`] it is moved to `server.log.old`, replacing the one before, and begun again.
pub const LOG_FILE: &str = "server.log";

/// How large the log grows before it is begun again.
const LOG_KEPT_BYTES: u64 = 8 * 1024 * 1024;

/// What the server printed: the last 300 lines, which the page shows, and the log file.
#[derive(Default)]
struct Lines {
    recent: VecDeque<String>,
    file: Option<LogFile>,
    /// A Yalain world's log has said where it listens, however many lines followed it.
    up: bool,
}

struct LogFile {
    path: PathBuf,
    file: std::fs::File,
    written: u64,
    limit: u64,
}

impl LogFile {
    fn open(path: PathBuf, limit: u64) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let written = file.metadata()?.len();
        Ok(Self {
            path,
            file,
            written,
            limit,
        })
    }

    /// Append `line`. A log that cannot be written never stops the server.
    fn write(&mut self, line: &str) {
        if self.written >= self.limit {
            let old = self.path.with_extension("log.old");
            let _ = std::fs::remove_file(&old);
            if std::fs::rename(&self.path, &old).is_ok() {
                if let Ok(fresh) = Self::open(self.path.clone(), self.limit) {
                    *self = fresh;
                }
            }
        }
        if writeln!(self.file, "{line}").is_ok() {
            self.written += line.len() as u64 + 1;
        }
    }
}

type Log = Arc<Mutex<Lines>>;
fn read_log(mut input: impl Read, log: Log) {
    let mut buf = [0; 4096];
    let mut line = Vec::new();
    loop {
        let count = match input.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for byte in &buf[..count] {
            if *byte == b'\n' {
                push_log(&log, &String::from_utf8_lossy(&line));
                line.clear();
            } else if line.len() < 8192 {
                line.push(*byte);
            }
        }
    }
    if !line.is_empty() {
        push_log(&log, &String::from_utf8_lossy(&line));
    }
}
fn push_log(log: &Log, line: &str) {
    let mut lines = log
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let line = crate::host_config::redact(line.trim_end_matches('\r'));
    if let Some(file) = lines.file.as_mut() {
        file.write(&line);
    }
    if lines.recent.len() == 300 {
        lines.recent.pop_front();
    }
    lines.up |= crate::host_yalain::is_up(&line);
    lines.recent.push_back(line);
}
fn save_log(dir: &Path, log: &Log) -> Result<(), String> {
    let lines: Vec<_> = log
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .recent
        .iter()
        .cloned()
        .collect();
    write_json(&dir.join("log.json"), &lines)
}
fn health(settings: &Settings) -> Option<u64> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();
    let bytes = agent
        .get(settings.status_url())
        .call()
        .ok()?
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_vec()
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if value["world_name"].as_str()? != settings.name {
        return None;
    }
    value["players_online"].as_u64()
}

struct Parent {
    id: Option<u32>,
    #[cfg(windows)]
    handle: windows_sys::Win32::Foundation::HANDLE,
}
impl Parent {
    fn new(id: Option<u32>) -> Self {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
            // The optional parent is the launcher that spawned this supervisor. A process handle
            // keeps a later process reusing the same numeric id from being mistaken for it.
            let handle = id.map_or(std::ptr::null_mut(), |pid| unsafe {
                OpenProcess(PROCESS_SYNCHRONIZE, 0, pid)
            });
            Self { id, handle }
        }
        #[cfg(not(windows))]
        {
            Self { id }
        }
    }
    fn alive(&self) -> bool {
        let Some(id) = self.id else {
            return true;
        };
        #[cfg(windows)]
        {
            let _ = id;
            // A null handle means the launcher exited before the supervisor opened it.
            !self.handle.is_null()
                && unsafe {
                    windows_sys::Win32::System::Threading::WaitForSingleObject(self.handle, 0)
                } == windows_sys::Win32::Foundation::WAIT_TIMEOUT
        }
        #[cfg(unix)]
        {
            unsafe extern "C" {
                fn getppid() -> i32;
            }
            // A supervisor is reparented when its launcher exits.
            unsafe { getppid() as u32 == id }
        }
        #[cfg(not(any(windows, unix)))]
        {
            let _ = id;
            true
        }
    }
}
#[cfg(windows)]
impl Drop for Parent {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.handle);
            }
        }
    }
}

/// What a world's page says of `server` ended by itself with `code` (`said` in words): Yalain's
/// code 3 is "the ports are taken", which it prints a `[ports-taken]` line for; anything else is
/// the code, and the log to look in.
fn ended(server: &str, code: Option<i32>, said: &str) -> String {
    if server == "Yalain" && code == Some(3) {
        return "Another program holds this world's ports. Choose another port in its Settings, \
                or close that program."
            .to_owned();
    }
    format!("{server} exited with {said}. Check the logs.")
}

/// What the server's console is given to stop: Empyrean's `exit`, and Yalain's `stop-now`, which
/// saves everyone in the world and exits at once (its `shutdown` counts two minutes down for the
/// players). Neither is killed: what players did since the last save would be lost.
fn stop_word(game: Game) -> &'static [u8] {
    match game {
        Game::Auberean => b"stop-now\n",
        Game::Dereth => b"exit\n",
    }
}

/// The world in `dir` as it starts: a downloaded update of its server's release put in first, then
/// its definition read, so the start runs the program the update put in (a Yalain release takes
/// the old one's place in the world's server folder).
fn updated_definition(dir: &Path) -> Result<Definition, String> {
    crate::host_updates::apply_pending(dir)?;
    read_definition(dir)
}

/// Runs without a web view. Stop asks the server to save and exit; it never kills it.
///
/// A world that updates by itself ([`crate::host_updates::automatic`]) is checked for updates while
/// it runs ([`watch_updates`]). Once one is downloaded the world counts down with its own
/// `shutdown`, the update goes in when it has stopped, and the server is started again, here, as
/// at first: a stop asked for meanwhile reaches the new server as it reached the old.
pub fn run(dir: &Path, stop: &AtomicBool, parent: Option<u32>) -> Result<(), String> {
    supervise(dir, stop, parent, |log| {
        let checkout = read_definition(dir).is_ok_and(|d| d.checkout);
        watch_updates(dir, checkout, log)
    })
}

/// [`run`], with the world's update checks as `updates` makes them.
fn supervise(
    dir: &Path,
    stop: &AtomicBool,
    parent: Option<u32>,
    updates: impl FnOnce(&Log) -> Updates,
) -> Result<(), String> {
    let parent = Parent::new(parent);
    let definition = read_definition(dir)?;
    let yalain = definition.game == Game::Auberean;
    let settings = read_settings(dir, definition.game)?;
    if yalain {
        crate::host_yalain::validate(&settings)?;
    } else {
        settings.validate()?;
        crate::host_config::validate_managed(
            &std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?,
            dir,
        )?;
    }
    let guard = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("run.lock"))
        .map_err(|e| e.to_string())?;
    guard
        .try_lock()
        .map_err(|_| "This world is already running.")?;
    let log: Log = Arc::new(Mutex::new(Lines {
        recent: VecDeque::new(),
        file: LogFile::open(dir.join(LOG_FILE), LOG_KEPT_BYTES).ok(),
        up: false,
    }));
    let updates = updates(&log);
    loop {
        let again = serve(dir, stop, &parent, &log, &updates)?;
        if !again || stop.load(Ordering::SeqCst) || !parent.alive() {
            return Ok(());
        }
    }
}

/// How long after the supervisor starts it first checks for an update, and how long it waits
/// between checks. `DERETH_HOST_UPDATE_SECONDS` sets both, for a test.
fn update_schedule() -> (Duration, Duration) {
    match std::env::var("DERETH_HOST_UPDATE_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|s| *s > 0)
    {
        Some(s) => (Duration::from_secs(s), Duration::from_secs(s)),
        None => (Duration::from_secs(300), Duration::from_secs(6 * 3600)),
    }
}

/// The world's own update checks, while its supervisor runs.
struct Updates {
    /// An update is downloaded and waits for the world to stop.
    ready: Arc<AtomicBool>,
    /// The version [`Self::ready`] was last raised for: a check while its countdown runs, or after
    /// the world has restarted into it, does not raise it again.
    raised: Arc<Mutex<Option<String>>>,
    /// The supervisor has ended: the checks end too.
    done: Arc<AtomicBool>,
}

impl Drop for Updates {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
    }
}

/// Checks for updates on [`update_schedule`] while the world in `dir` is set to update by itself,
/// as the launcher checks while it is open, and says when one is downloaded. A local build's
/// world is never updated from a release.
fn watch_updates(dir: &Path, checkout: bool, log: &Log) -> Updates {
    let updates = Updates {
        ready: Arc::new(AtomicBool::new(false)),
        raised: Arc::new(Mutex::new(None)),
        done: Arc::new(AtomicBool::new(false)),
    };
    if checkout {
        return updates;
    }
    let (first, every) = update_schedule();
    let (ready, raised, done) = (
        updates.ready.clone(),
        updates.raised.clone(),
        updates.done.clone(),
    );
    let (dir, checks_log) = (dir.to_path_buf(), log.clone());
    let wait = move |how_long: Duration| {
        let until = Instant::now() + how_long;
        while Instant::now() < until {
            if done.load(Ordering::SeqCst) {
                return false;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        !done.load(Ordering::SeqCst)
    };
    let spawned = std::thread::Builder::new()
        .name("updates".into())
        .spawn(move || {
            if !wait(first) {
                return;
            }
            loop {
                if crate::host_updates::automatic(&dir) {
                    if let Err(error) = crate::host_updates::check(&dir) {
                        push_log(
                            &checks_log,
                            &format!("Could not check for updates: {error}"),
                        );
                    }
                    let view = crate::host_updates::view(&dir);
                    let mut raised = raised
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if raise(&view, &mut raised) {
                        ready.store(true, Ordering::SeqCst);
                    }
                }
                if !wait(every) {
                    return;
                }
            }
        });
    if let Err(error) = spawned {
        push_log(log, &format!("Could not watch for updates: {error}"));
    }
    updates
}

/// Whether a check that found `view` begins the world's countdown: an update is downloaded, and it
/// is not the one the countdown was last begun for (`raised`, which this then names).
fn raise(view: &crate::host_updates::View, raised: &mut Option<String>) -> bool {
    if !view.pending || *raised == view.version {
        return false;
    }
    raised.clone_from(&view.version);
    true
}

/// One run of the world's server: started, supervised, and stopped. Whether to start it again,
/// because it stopped for its own update.
fn serve(
    dir: &Path,
    stop: &AtomicBool,
    parent: &Parent,
    log: &Log,
    updates: &Updates,
) -> Result<bool, String> {
    let definition = read_definition(dir)?;
    let yalain = definition.game == Game::Auberean;
    let settings = read_settings(dir, definition.game)?;
    let server = if yalain { "Yalain" } else { "Empyrean" };
    let log = log.clone();
    let definition = updated_definition(dir).inspect_err(|error| {
        let _ = write_json(
            &dir.join("status.json"),
            &Status {
                phase: "failed".into(),
                message: Some(error.clone()),
                ..Default::default()
            },
        );
    })?;
    let mut state = Status {
        phase: "starting".into(),
        pid: Some(std::process::id()),
        ..Status::default()
    };
    write_json(&dir.join("status.json"), &state)?;
    log.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .up = false;
    let args: Vec<std::ffi::OsString> = if yalain {
        crate::host_yalain::args(dir)
    } else {
        vec!["--config".into(), dir.join("empyrean.toml").into()]
    };
    let mut child = crate::components::command(&definition.program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            let message = format!("Could not start {server}: {e}");
            state.phase = "failed".into();
            state.pid = None;
            state.message = Some(message.clone());
            let _ = write_json(&dir.join("status.json"), &state);
            message
        })?;
    state.started = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    );
    // Commands queued before this start were for a world that was not running.
    let _ = take_commands(dir);
    let mut stdin = child.stdin.take();
    let stdout = child.stdout.take().ok_or("No server output")?;
    let stderr = child.stderr.take().ok_or("No server error output")?;
    let stdout_log = log.clone();
    let stderr_log = log.clone();
    let out = std::thread::spawn(move || read_log(stdout, stdout_log));
    let err = std::thread::spawn(move || read_log(stderr, stderr_log));
    let mut stopping = false;
    // While the world counts down to an update: when it began, and what its status says.
    let mut countdown: Option<(Instant, String)> = None;
    let countdown_limit =
        Duration::from_secs(u64::try_from(settings.shutdown_warning.max(0)).unwrap_or(0) + 180);
    let started = Instant::now();
    let mut next_status = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(exit)) => break exit,
            Ok(None) => {}
            Err(error) => {
                state.message = Some(format!("Could not check {server}'s process: {error}"))
            }
        }
        if !stopping {
            for line in take_commands(dir) {
                push_log(&log, &shown_command(&line));
                if let Some(input) = stdin.as_mut() {
                    if let Err(error) = writeln!(input, "{line}").and_then(|()| input.flush()) {
                        push_log(&log, &format!("Could not pass the command on: {error}"));
                    }
                }
            }
        }
        if !stopping && countdown.is_none() && updates.ready.swap(false, Ordering::SeqCst) {
            let version = crate::host_updates::view(dir).version.unwrap_or_default();
            let line = format!("shutdown Updating to {server} {version}.");
            push_log(&log, &shown_command(&line));
            if let Some(input) = stdin.as_mut() {
                if let Err(error) = writeln!(input, "{line}").and_then(|()| input.flush()) {
                    push_log(
                        &log,
                        &format!("Could not begin the update's countdown: {error}"),
                    );
                }
            }
            countdown = Some((
                Instant::now(),
                format!("Updating to {server} {version}: restarting after the countdown."),
            ));
        }
        // A countdown still going long after it should have ended was canceled at the console: the
        // update goes in at the next check, or the next stop.
        if countdown
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() > countdown_limit)
        {
            push_log(&log, "The update's countdown was canceled.");
            countdown = None;
            // The next check raises it again.
            *updates
                .raised
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }
        let note = countdown.as_ref().map(|(_, note)| note.clone());
        if !stopping
            && (stop.load(Ordering::SeqCst) || !parent.alive() || dir.join("stop.request").exists())
        {
            if let Some(mut stdin) = stdin.take() {
                let word = stop_word(definition.game);
                if let Err(error) = stdin.write_all(word).and_then(|()| stdin.flush()) {
                    push_log(&log, &format!("Could not request shutdown: {error}"));
                }
            }
            stopping = true;
            state.phase = "stopping".into();
            state.message = Some("Saving and stopping...".into());
        }
        if Instant::now() >= next_status {
            #[cfg(windows)]
            if stopping {
                crate::host_service::stopping_progress();
            }
            if !stopping && yalain {
                // Up once its log has said where it listens (the world database's notes may
                // follow by the thousand), or its status endpoint answers, where its players are
                // counted as Empyrean's are.
                let players = health(&settings);
                let up = log
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .up;
                if up || players.is_some() {
                    state.phase = "running".into();
                    state.players = players;
                    state.message.clone_from(&note);
                } else if started.elapsed() > Duration::from_secs(300) {
                    state.message = Some("Waiting for the world to start. Check its log.".into());
                }
            } else if !stopping {
                if let Some(players) = health(&settings) {
                    state.phase = "running".into();
                    state.players = Some(players);
                    state.message.clone_from(&note);
                } else if started.elapsed() > Duration::from_secs(120) {
                    state.message = Some("Waiting for the world to start. Check its log.".into());
                }
            }
            // An unwritable status file must never abandon the running server.
            let _ = write_json(&dir.join("status.json"), &state);
            let _ = save_log(dir, &log);
            next_status = Instant::now() + Duration::from_secs(1);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let _ = out.join();
    let _ = err.join();
    state.pid = None;
    state.players = None;
    state.started = None;
    let _ = take_commands(dir);
    // A Yalain world stopped by the launcher ends when it is told to: that is a clean stop.
    let clean = result.success() || (yalain && stopping);
    state.phase = if clean { "stopped" } else { "failed" }.into();
    state.message = if clean {
        None
    } else {
        Some(ended(server, result.code(), &result.to_string()))
    };
    let update_result = if clean {
        crate::host_updates::apply_pending(dir)
    } else {
        Ok(())
    };
    // The world stopped for its own update, and nobody asked it to stop: it starts again, on the
    // new release or, when that did not go in, on the one it ran.
    let restart = clean && countdown.is_some() && !stopping;
    if restart {
        if let Err(error) = &update_result {
            push_log(&log, &format!("The update did not go in: {error}"));
            crate::host_updates::skip_failed(dir);
        }
        state.phase = "starting".into();
        state.message = None;
        let _ = std::fs::remove_file(dir.join("stop.request"));
        save_log(dir, &log)?;
        write_json(&dir.join("status.json"), &state)?;
        return Ok(true);
    }
    if let Err(error) = &update_result {
        state.phase = "failed".into();
        state.message = Some(error.clone());
    }
    let _ = std::fs::remove_file(dir.join("stop.request"));
    save_log(dir, &log)?;
    write_json(&dir.join("status.json"), &state)?;
    if clean {
        update_result.map(|()| false)
    } else {
        Err(state.message.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Yalain world whose update to 0.1.1 was downloaded starts the program that update put in
    /// (in its server folder), not the one it ran before: the definition is read once the update
    /// is in.
    #[test]
    fn a_yalain_world_starts_the_release_its_update_just_put_in() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let exe = format!("yalain{}", std::env::consts::EXE_SUFFIX);
        let old = dir.join("runtime/0.1.0").join(&exe);
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, b"the old program").unwrap();
        write_json(
            &dir.join("host.json"),
            &Definition {
                id: "host-test".into(),
                version: "0.1.0".into(),
                program: old.clone(),
                wrapper: dir.join("wrapper"),
                checkout: false,
                game: Game::Auberean,
            },
        )
        .unwrap();
        std::fs::write(dir.join(crate::host_yalain::CONFIG), "[server]\n").unwrap();
        let download = dir.join("update-download");
        let new = download.join("yalain-0.1.1").join(&exe);
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        std::fs::write(&new, b"the new program").unwrap();
        std::fs::write(
            download.join("checked-release.json"),
            serde_json::json!({
                "version": "0.1.1",
                "upgrade": { "kind": "patch", "previous": "0.1.0", "upgrades_from": "0.1.0",
                             "world_database": "v0.23.0", "world_database_fetch": false },
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.join("update.json"),
            serde_json::json!({ "pending": true, "notes": "", "rollback": false }).to_string(),
        )
        .unwrap();
        assert_eq!(read_definition(dir).unwrap().program, old);
        let starting = updated_definition(dir).unwrap();
        assert_eq!(starting.version, "0.1.1");
        assert!(starting.program.starts_with(dir.join(SERVER_DIR)));
        assert_eq!(
            std::fs::read(&starting.program).unwrap(),
            b"the new program"
        );
        // Nothing pending: the definition as it is.
        assert_eq!(updated_definition(dir).unwrap().program, starting.program);
    }
    /// A world made when each release had a folder of its own moves into its server folder once,
    /// before it starts; an Empyrean world whose updater keeps a record of an update in its folder
    /// stays where the record names it, and so does a world on this checkout's build.
    #[test]
    fn a_world_moves_into_its_one_server_folder_once_unless_a_record_names_where_it_is() {
        let world = |game: Game, checkout: bool| {
            let temp = tempfile::tempdir().unwrap();
            let dir = temp.path();
            let program = dir.join("runtime/0.1.0/server.exe");
            std::fs::create_dir_all(program.parent().unwrap()).unwrap();
            std::fs::write(&program, b"the program").unwrap();
            write_json(
                &dir.join("host.json"),
                &Definition {
                    id: "host-test".into(),
                    version: "0.1.0".into(),
                    program,
                    wrapper: dir.join("wrapper"),
                    checkout,
                    game,
                },
            )
            .unwrap();
            temp
        };
        for game in [Game::Dereth, Game::Auberean] {
            let temp = world(game, false);
            let dir = temp.path();
            let moved = settle_server(dir).unwrap();
            assert_eq!(moved.program, dir.join("server/server.exe"));
            assert_eq!(read_definition(dir).unwrap().program, moved.program);
            assert_eq!(std::fs::read(&moved.program).unwrap(), b"the program");
            assert!(!dir.join("runtime/0.1.0").exists());
            assert_eq!(settle_server(dir).unwrap().program, moved.program, "once");
        }

        let temp = world(Game::Dereth, false);
        let dir = temp.path();
        std::fs::create_dir(dir.join("runtime/0.1.0/.empyrean-update")).unwrap();
        let kept = settle_server(dir).unwrap();
        assert_eq!(kept.program, dir.join("runtime/0.1.0/server.exe"));
        assert!(!dir.join(SERVER_DIR).exists());

        let temp = world(Game::Auberean, true);
        let dir = temp.path();
        assert_eq!(
            settle_server(dir).unwrap().program,
            dir.join("runtime/0.1.0/server.exe")
        );
    }

    #[test]
    fn a_missing_server_reports_failure_and_releases_the_host_lock() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        write_json(
            &dir.join("host.json"),
            &Definition {
                id: "host-test".into(),
                version: "1.0.0".into(),
                program: dir.join("missing-server"),
                wrapper: dir.join("wrapper"),
                checkout: false,
                game: dereth_launch::world::Game::Dereth,
            },
        )
        .unwrap();
        let settings = Settings {
            dats: dir.to_path_buf(),
            ..Settings::default()
        };
        std::fs::write(
            dir.join("empyrean.toml"),
            settings.update_config("").unwrap(),
        )
        .unwrap();
        assert!(run(dir, &AtomicBool::new(false), None).is_err());
        assert!(!active(dir));
        let state = status(dir);
        assert_eq!(state.phase, "failed");
        assert!(state
            .message
            .unwrap()
            .starts_with("Could not start Empyrean:"));
        assert!(state.pid.is_none());
    }
    /// A stand-in server in `dir`: it notes each start in `starts.txt`, and exits once its console
    /// says `shutdown` or `exit`, as a world does once its countdown ends or it is told to stop.
    fn stand_in_server(dir: &Path) -> PathBuf {
        #[cfg(windows)]
        let (name, text) = (
            "server.cmd",
            concat!(
                "@echo off\r\n",
                "echo started>> \"%~dp0starts.txt\"\r\n",
                ":again\r\n",
                "set \"line=\"\r\n",
                "set /p line=\r\n",
                "if \"%line:~0,8%\"==\"shutdown\" exit /b 0\r\n",
                "if \"%line%\"==\"exit\" exit /b 0\r\n",
                "goto again\r\n",
            ),
        );
        #[cfg(not(windows))]
        let (name, text) = (
            "server.sh",
            concat!(
                "#!/bin/sh\n",
                "echo started >> \"$(dirname \"$0\")/starts.txt\"\n",
                "while read -r line; do\n",
                "  case \"$line\" in shutdown*|exit) exit 0 ;; esac\n",
                "done\n",
            ),
        );
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    fn starts(dir: &Path) -> usize {
        std::fs::read_to_string(dir.join("starts.txt"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    /// A world that updates by itself and has an update waiting counts down with its own
    /// `shutdown`, starts again once it has stopped, and a stop asked for then reaches the server
    /// started again: the supervisor ends, and the world is not left running.
    #[test]
    fn a_world_restarts_after_its_update_countdown_and_a_stop_then_reaches_the_new_server() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        write_json(
            &dir.join("host.json"),
            &Definition {
                id: "host-test".into(),
                version: "1.0.0".into(),
                program: stand_in_server(dir),
                wrapper: dir.join("wrapper"),
                // A local build's: nothing is installed, so the start runs the same server.
                checkout: true,
                game: dereth_launch::world::Game::Dereth,
            },
        )
        .unwrap();
        let settings = Settings {
            dats: dir.to_path_buf(),
            ..Settings::default()
        };
        std::fs::write(
            dir.join("empyrean.toml"),
            settings.update_config("").unwrap(),
        )
        .unwrap();
        let stop = AtomicBool::new(false);
        let result = std::thread::scope(|scope| {
            let supervisor = scope.spawn(|| {
                supervise(dir, &stop, None, |_| Updates {
                    ready: Arc::new(AtomicBool::new(true)),
                    raised: Arc::new(Mutex::new(None)),
                    done: Arc::new(AtomicBool::new(false)),
                })
            });
            let waited = Instant::now();
            while starts(dir) < 2 && waited.elapsed() < Duration::from_secs(60) {
                std::thread::sleep(Duration::from_millis(50));
            }
            stop.store(true, Ordering::SeqCst);
            supervisor.join().unwrap()
        });
        assert_eq!(result, Ok(()));
        assert_eq!(starts(dir), 2, "started once, and again after the update");
        assert!(!active(dir));
        assert_eq!(status(dir).phase, "stopped");
        let log = std::fs::read_to_string(dir.join(LOG_FILE)).unwrap();
        assert!(log.contains("> shutdown Updating to Empyrean"), "{log}");
    }

    /// A check begins the countdown once for each downloaded update: not again while it counts
    /// down, nor once the world has restarted into it, and again for the next one.
    #[test]
    fn a_downloaded_update_begins_one_countdown() {
        let view = |pending, version: &str| crate::host_updates::View {
            pending,
            version: Some(version.into()),
            ..Default::default()
        };
        let mut raised = None;
        assert!(
            !raise(&view(false, "0.3.10"), &mut raised),
            "offered, not downloaded"
        );
        assert!(raise(&view(true, "0.3.10"), &mut raised));
        assert!(!raise(&view(true, "0.3.10"), &mut raised), "counting down");
        assert!(
            !raise(&view(false, "0.3.10"), &mut raised),
            "restarted into it"
        );
        assert!(raise(&view(true, "0.3.11"), &mut raised), "the next one");
        raised = None;
        assert!(
            raise(&view(true, "0.3.11"), &mut raised),
            "a canceled countdown is begun again"
        );
    }

    /// A world is asked to save and stop through its console, in its server's own word.
    #[test]
    fn a_yalain_that_finds_its_ports_taken_says_so_and_any_other_end_names_its_code() {
        assert!(ended("Yalain", Some(3), "exit code: 3").starts_with("Another program holds"));
        assert_eq!(
            ended("Yalain", Some(1), "exit code: 1"),
            "Yalain exited with exit code: 1. Check the logs."
        );
        assert_eq!(
            ended("Empyrean", Some(3), "exit code: 3"),
            "Empyrean exited with exit code: 3. Check the logs."
        );
    }

    #[test]
    fn a_stopping_world_is_asked_to_save_and_exit_in_its_servers_own_word() {
        assert_eq!(stop_word(Game::Auberean), b"stop-now\n");
        assert_eq!(stop_word(Game::Dereth), b"exit\n");
    }
    #[test]
    fn console_commands_wait_in_order_for_a_running_world_and_hide_passwords_in_the_log() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        assert_eq!(
            queue_command(dir, "gamecast hello").unwrap_err(),
            "Start the world first."
        );
        // A running world holds its lock.
        let guard = std::fs::File::create(dir.join("run.lock")).unwrap();
        guard.try_lock().unwrap();
        assert!(queue_command(dir, "  ").is_err());
        assert!(queue_command(
            dir, "a
b"
        )
        .is_err());
        for line in [
            "set-shutdown-interval 300",
            "shutdown",
            "accountcreate friend s3cret",
        ] {
            queue_command(dir, line).unwrap();
        }
        assert_eq!(
            take_commands(dir),
            [
                "set-shutdown-interval 300",
                "shutdown",
                "accountcreate friend s3cret"
            ]
        );
        assert!(take_commands(dir).is_empty(), "each is passed on once");
        assert_eq!(
            shown_command("accountcreate friend s3cret 5"),
            "> accountcreate friend *** 5"
        );
        assert_eq!(
            shown_command("@set-accountpassword a b"),
            "> @set-accountpassword a ***"
        );
        assert_eq!(shown_command("gamecast  hi there"), "> gamecast hi there");
    }
    #[test]
    fn captured_logs_are_bounded_and_redacted_including_unterminated_lines() {
        let log: Log = Arc::new(Mutex::new(Lines::default()));
        let text = (0..400)
            .map(|i| format!("line {i}\r\n"))
            .collect::<String>()
            + "password=hidden";
        read_log(text.as_bytes(), log.clone());
        let lines = &log.lock().unwrap().recent;
        assert_eq!(lines.len(), 300);
        assert_eq!(lines.front().unwrap(), "line 101");
        assert_eq!(lines.back().unwrap(), "[credential details hidden]");
    }
    /// A Yalain world is up once its log has said where it listens, even when the world
    /// database's notes push that line out of the lines the page keeps.
    #[test]
    fn a_yalain_world_is_up_once_its_log_said_so_however_many_lines_followed() {
        let log: Log = Arc::new(Mutex::new(Lines {
            recent: VecDeque::new(),
            file: None,
            up: false,
        }));
        push_log(&log, "the level table read");
        assert!(!log.lock().unwrap().up);
        push_log(
            &log,
            "2026-11-01T10:00:00Z  INFO yalain: listening on 127.0.0.1:9200/9201 (login) and \
             127.0.0.1:9202/9203 (world, referred as 127.0.0.1) [listening]",
        );
        for i in 0..1000 {
            push_log(&log, &format!("not handled yet what=a camp (c-{i})"));
        }
        let lines = log.lock().unwrap();
        assert!(lines.up);
        assert!(!lines.recent.iter().any(|l| l.contains("listening on")));
    }

    #[test]
    fn the_log_file_keeps_every_line_and_begins_again_once_it_is_large() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(LOG_FILE);
        let log: Log = Arc::new(Mutex::new(Lines {
            recent: VecDeque::new(),
            file: Some(LogFile::open(path.clone(), 40).unwrap()),
            up: false,
        }));
        let text = (0..6)
            .map(|i| format!("line {i} of six\n"))
            .collect::<String>();
        read_log((text + "token=abc\n").as_bytes(), log.clone());
        let old = std::fs::read_to_string(temp.path().join("server.log.old")).unwrap();
        let new = std::fs::read_to_string(&path).unwrap();
        assert_eq!(old.lines().count() + new.lines().count(), 4);
        assert_eq!(new.lines().last(), Some("[credential details hidden]"));
        assert!(!new.contains("abc") && !old.contains("abc"));
        assert_eq!(log.lock().unwrap().recent.len(), 7);
    }
}
