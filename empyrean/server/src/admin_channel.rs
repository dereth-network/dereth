//! Not ACE (V456): the admin channel, the server's console for a program on the same computer.
//!
//! A world that runs as a service has no one at its console, and ACE gives no other way in. With
//! `server.admin_channel` on (the default), the server listens on loopback, on a port the system
//! picks, and writes where and with what secret to `admin-channel.json` in its own folder (the
//! configuration file's, else the working directory): `{"address", "secret", "pid"}`. A program
//! that can read that file, the Dereth launcher among them, can then type console commands and
//! read what the console shows, as a person at the console would. The file is removed when the
//! server stops, and a file left by a server that did not stop cleanly names a process that is no
//! longer there.
//!
//! **Who may use it.** Whoever can read the world's own folder, and no one else: the secret is 32
//! random bytes drawn at each start, kept nowhere but in that file, which on Linux and macOS is
//! readable by the server's own user alone; on Windows it has the folder's permissions. The
//! listener takes connections from loopback only. A connection's first line must be the secret,
//! within [`SECRET_WAIT`]; anything else closes it without a word, so a web page that makes a
//! browser connect to the port learns nothing. At most [`MAX_CLIENTS`] are connected at once.
//!
//! **The conversation.** Lines of UTF-8 both ways. After the secret, each line the program sends
//! is a console command, run as one typed at the console is (an admin's, as the console's always
//! are); a line over [`MAX_LINE`] bytes closes the connection. The server sends JSON, one object a
//! line: first `{"hello": "empyrean", "version", "world"}`, then the console's last
//! [`BACKLOG`] lines, then each line the console shows as it is shown, `{"line": "..."}`: replies
//! and log lines alike. A program that does not read its lines quickly enough is dropped rather
//! than let the console wait for it.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use empyrean_command::command_manager::{self, WorldCommand};
use serde::{Deserialize, Serialize};

/// The file the channel's address and secret are written to, in the server's folder.
pub const FILE_NAME: &str = "admin-channel.json";

/// The console lines a program is sent when it connects.
pub const BACKLOG: usize = 200;

/// The most programs connected at once.
pub const MAX_CLIENTS: usize = 4;

/// How long a connection has to send the secret.
pub const SECRET_WAIT: Duration = Duration::from_secs(5);

/// The longest line a program may send, in bytes.
pub const MAX_LINE: usize = 4096;

/// The lines waiting for one program; a program further behind than this is dropped.
const QUEUE: usize = 4096;

/// What `admin-channel.json` holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelFile {
    /// Where the channel listens: a loopback address and its port.
    pub address: String,
    /// The secret a connection's first line must be: 64 hexadecimal digits.
    pub secret: String,
    /// The server's process id.
    pub pid: u32,
}

/// The console's lines, kept for programs that connect and handed to those connected.
#[derive(Debug, Default)]
struct Output {
    backlog: VecDeque<String>,
    readers: Vec<SyncSender<String>>,
}

impl Output {
    fn push(&mut self, line: &str) {
        if self.backlog.len() == BACKLOG {
            self.backlog.pop_front();
        }
        self.backlog.push_back(line.to_owned());
        // A reader that has gone, or fallen too far behind, is dropped: its connection ends.
        self.readers.retain(|r| {
            !matches!(
                r.try_send(line.to_owned()),
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_))
            )
        });
    }
}

/// The connections open, each by its number, so the channel can end them and count them.
type Open = Arc<Mutex<Vec<(u64, TcpStream)>>>;

/// The running channel. Dropping it (or [`AdminChannel::shutdown`]) stops it and removes its
/// file.
#[derive(Debug)]
pub struct AdminChannel {
    addr: SocketAddr,
    file: PathBuf,
    stop: Arc<AtomicBool>,
    open: Open,
    accept: Option<JoinHandle<()>>,
}

impl AdminChannel {
    /// Where it listens.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The file its address and secret are in.
    #[must_use]
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Stops listening, ends every connection and removes the file.
    pub fn shutdown(mut self) {
        self.stop_now();
    }

    fn stop_now(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.accept.take() {
            let _ = t.join();
        }
        for (_, s) in self
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
        {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
        empyrean_command::console::set_tap(None);
        let _ = std::fs::remove_file(&self.file);
    }
}

impl Drop for AdminChannel {
    fn drop(&mut self) {
        if !self.stop.load(Ordering::SeqCst) {
            self.stop_now();
        }
    }
}

/// A new secret: 32 random bytes from the system, as 64 lowercase hexadecimal digits.
fn new_secret() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Whether `given` is `secret`, compared in time that does not depend on where they differ.
fn same_secret(given: &[u8], secret: &[u8]) -> bool {
    given.len() == secret.len()
        && given
            .iter()
            .zip(secret)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// Writes `file` so that only this user may read it (on Linux and macOS), whole or not at all.
fn write_private(file: &Path, text: &str) -> std::io::Result<()> {
    let tmp = file.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, file)
}

/// Starts the channel for the server whose folder is `folder`: listens on loopback, writes the
/// file, and from then on hands console commands to the world thread through `commands`. `hello`
/// is what a program is told first (the server's version and world name).
///
/// # Errors
/// When the port cannot be bound or the file cannot be written; nothing is left listening then.
pub fn start(
    folder: &Path,
    commands: Sender<WorldCommand>,
    hello: serde_json::Value,
) -> std::io::Result<AdminChannel> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    let secret = new_secret()?;
    let file = folder.join(FILE_NAME);
    let record = ChannelFile {
        address: addr.to_string(),
        secret: secret.clone(),
        pid: std::process::id(),
    };
    write_private(
        &file,
        &serde_json::to_string_pretty(&record).unwrap_or_default(),
    )?;
    let output = Arc::new(Mutex::new(Output::default()));
    {
        let output = Arc::clone(&output);
        empyrean_command::console::set_tap(Some(Box::new(move |line: &str| {
            output
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(line);
        })));
    }
    let stop = Arc::new(AtomicBool::new(false));
    let open: Open = Arc::new(Mutex::new(Vec::new()));
    let accept = {
        let stop = Arc::clone(&stop);
        let open = Arc::clone(&open);
        std::thread::Builder::new()
            .name("Admin channel".into())
            .spawn(move || {
                let hello = format!("{hello}\n");
                let mut next = 0u64;
                while !stop.load(Ordering::SeqCst) {
                    let (stream, peer) = match listener.accept() {
                        Ok(accepted) => accepted,
                        Err(_) => {
                            std::thread::sleep(Duration::from_millis(50));
                            continue;
                        }
                    };
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    next += 1;
                    let id = next;
                    {
                        let mut open = open.lock().unwrap_or_else(PoisonError::into_inner);
                        let Ok(handle) = stream.try_clone() else {
                            continue;
                        };
                        if open.len() >= MAX_CLIENTS {
                            continue;
                        }
                        open.push((id, handle));
                    }
                    let connection = Connection {
                        secret: secret.clone(),
                        output: Arc::clone(&output),
                        hello: hello.clone(),
                        commands: commands.clone(),
                    };
                    let ended = Arc::clone(&open);
                    let spawned = std::thread::Builder::new()
                        .name("Admin channel connection".into())
                        .spawn(move || {
                            connection.serve(stream);
                            open_remove(&ended, id);
                        });
                    if spawned.is_err() {
                        open_remove(&open, id);
                    }
                }
            })?
    };
    Ok(AdminChannel {
        addr,
        file,
        stop,
        open,
        accept: Some(accept),
    })
}

/// Forgets connection `id` once it has ended.
fn open_remove(open: &Open, id: u64) {
    open.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .retain(|(n, _)| *n != id);
}

/// What one connection needs.
struct Connection {
    secret: String,
    output: Arc<Mutex<Output>>,
    hello: String,
    commands: Sender<WorldCommand>,
}

/// Reads one line of at most [`MAX_LINE`] bytes, without its end; `None` at the end of the input,
/// on an error, or for a line too long.
fn read_line(reader: &mut impl BufRead) -> Option<String> {
    let mut line = Vec::new();
    let limit = u64::try_from(MAX_LINE + 2).unwrap_or(u64::MAX);
    let n = reader.take(limit).read_until(b'\n', &mut line).ok()?;
    if n == 0 || (line.last() != Some(&b'\n') && line.len() > MAX_LINE) {
        return None;
    }
    while matches!(line.last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    Some(String::from_utf8_lossy(&line).into_owned())
}

impl Connection {
    /// The secret, then lines both ways until either side ends.
    fn serve(self, stream: TcpStream) {
        let _ = stream.set_nonblocking(false);
        if stream.set_read_timeout(Some(SECRET_WAIT)).is_err() {
            return;
        }
        let Ok(read_half) = stream.try_clone() else {
            return;
        };
        let mut reader = BufReader::new(read_half);
        let given = read_line(&mut reader).unwrap_or_default();
        if !same_secret(given.as_bytes(), self.secret.as_bytes()) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return;
        }
        let _ = stream.set_read_timeout(None);
        let (tx, rx) = mpsc::sync_channel::<String>(QUEUE);
        let backlog: Vec<String> = {
            let mut out = self.output.lock().unwrap_or_else(PoisonError::into_inner);
            out.readers.push(tx);
            out.backlog.iter().cloned().collect()
        };
        let Ok(write_half) = stream.try_clone() else {
            return;
        };
        let ended = Arc::new(AtomicBool::new(false));
        let writer = {
            let ended = Arc::clone(&ended);
            let hello = self.hello;
            std::thread::spawn(move || write_lines(write_half, &hello, backlog, &rx, &ended))
        };
        let commands = self.commands;
        while let Some(line) = read_line(&mut reader) {
            if !command_manager::submit_console_line(&line, &|c| commands.send(c).is_ok()) {
                break;
            }
        }
        ended.store(true, Ordering::SeqCst);
        let _ = stream.shutdown(std::net::Shutdown::Both);
        let _ = writer.join();
    }
}

/// Writes the hello, the backlog and then each console line as it comes, until the connection
/// ends (`ended`, or a write that fails) or the console's side of it does.
fn write_lines(
    mut to: TcpStream,
    hello: &str,
    backlog: Vec<String>,
    rx: &Receiver<String>,
    ended: &AtomicBool,
) {
    let line = |text: &str| format!("{}\n", serde_json::json!({ "line": text }));
    if to.write_all(hello.as_bytes()).is_err() {
        return;
    }
    for text in backlog {
        if to.write_all(line(&text).as_bytes()).is_err() {
            return;
        }
    }
    while !ended.load(Ordering::SeqCst) {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(text) => {
                if to.write_all(line(&text).as_bytes()).is_err() {
                    return;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// The channel's file in `folder`, read; `None` when there is none, or it does not read.
#[must_use]
pub fn read_file(folder: &Path) -> Option<ChannelFile> {
    let mut text = String::new();
    std::fs::File::open(folder.join(FILE_NAME))
        .ok()?
        .read_to_string(&mut text)
        .ok()?;
    serde_json::from_str(&text).ok()
}

/// Removes a channel file a server left behind, as one that is off leaves none.
pub fn remove_stale(folder: &Path) {
    let _ = std::fs::remove_file(folder.join(FILE_NAME));
}
