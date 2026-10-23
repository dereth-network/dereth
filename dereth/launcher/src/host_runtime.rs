//! A host supervisor used by the launcher and native service managers.
//!
//! The supervisor owns the server's console: it writes the server's output to the log, and passes
//! it the commands the launcher queues in the world's `console` folder ([`queue_command`]), one
//! file a line, so a world running in the background takes them as one the launcher started does.

use crate::host_config::Settings;
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
        .get(format!("http://127.0.0.1:{}/status", settings.status_port))
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

/// Runs without a web view. Stop asks the server to save and exit; it never kills it.
pub fn run(dir: &Path, stop: &AtomicBool, parent: Option<u32>) -> Result<(), String> {
    let parent = Parent::new(parent);
    let definition = read_definition(dir)?;
    let settings = Settings::from_config(
        &std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?,
    )?;
    settings.validate()?;
    crate::host_config::validate_managed(
        &std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?,
        dir,
    )?;
    let guard = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("run.lock"))
        .map_err(|e| e.to_string())?;
    guard
        .try_lock()
        .map_err(|_| "This world is already running.")?;
    crate::host_updates::apply_pending(dir).inspect_err(|error| {
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
    let log: Log = Arc::new(Mutex::new(Lines {
        recent: VecDeque::new(),
        file: LogFile::open(dir.join(LOG_FILE), LOG_KEPT_BYTES).ok(),
    }));
    let mut child = crate::components::command(&definition.program)
        .args([
            "--config",
            dir.join("empyrean.toml")
                .to_str()
                .ok_or("Invalid configuration path")?,
        ])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            let message = format!("Could not start Empyrean: {e}");
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
    let started = Instant::now();
    let mut next_status = Instant::now();
    let result = loop {
        match child.try_wait() {
            Ok(Some(exit)) => break exit,
            Ok(None) => {}
            Err(error) => {
                state.message = Some(format!("Could not check Empyrean's process: {error}"))
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
        if !stopping
            && (stop.load(Ordering::SeqCst) || !parent.alive() || dir.join("stop.request").exists())
        {
            if let Some(mut stdin) = stdin.take() {
                if let Err(error) = stdin.write_all(b"exit\n").and_then(|()| stdin.flush()) {
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
            if !stopping {
                if let Some(players) = health(&settings) {
                    state.phase = "running".into();
                    state.players = Some(players);
                    state.message = None;
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
    state.phase = if result.success() {
        "stopped"
    } else {
        "failed"
    }
    .into();
    state.message = if result.success() {
        None
    } else {
        Some(format!("Empyrean exited with {result}. Check the logs."))
    };
    let update_result = if result.success() {
        crate::host_updates::apply_pending(dir)
    } else {
        Ok(())
    };
    if let Err(error) = &update_result {
        state.phase = "failed".into();
        state.message = Some(error.clone());
    }
    let _ = std::fs::remove_file(dir.join("stop.request"));
    save_log(dir, &log)?;
    write_json(&dir.join("status.json"), &state)?;
    if result.success() {
        update_result
    } else {
        Err(state.message.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    #[test]
    fn the_log_file_keeps_every_line_and_begins_again_once_it_is_large() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(LOG_FILE);
        let log: Log = Arc::new(Mutex::new(Lines {
            recent: VecDeque::new(),
            file: Some(LogFile::open(path.clone(), 40).unwrap()),
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
