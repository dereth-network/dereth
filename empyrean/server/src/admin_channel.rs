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
    readers: Vec<Reader>,
}

/// A connected program's queue of lines, and its connection, to end when it falls too far behind.
#[derive(Debug)]
struct Reader {
    lines: SyncSender<String>,
    connection: TcpStream,
}

impl Output {
    fn push(&mut self, line: &str) {
        if self.backlog.len() == BACKLOG {
            self.backlog.pop_front();
        }
        self.backlog.push_back(line.to_owned());
        // A reader that has gone, or fallen too far behind, is dropped and its connection ended,
        // which frees its place.
        self.readers
            .retain(|r| match r.lines.try_send(line.to_owned()) {
                Ok(()) => true,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                    let _ = r.connection.shutdown(std::net::Shutdown::Both);
                    false
                }
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
    /// The console's lines and their readers, which a test looks at.
    #[cfg(test)]
    output: Arc<Mutex<Output>>,
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
        let output = Arc::clone(&output);
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
                    let place = Place {
                        open: Arc::clone(&open),
                        id,
                    };
                    // A thread that cannot be started gives the place back as it is dropped.
                    let _ = std::thread::Builder::new()
                        .name("Admin channel connection".into())
                        .spawn(move || {
                            let _place = place;
                            connection.serve(stream);
                        });
                }
            })?
    };
    Ok(AdminChannel {
        addr,
        file,
        stop,
        open,
        #[cfg(test)]
        output,
        accept: Some(accept),
    })
}

/// A connection's place among the open ones, given back however its thread ends.
struct Place {
    open: Open,
    id: u64,
}

impl Drop for Place {
    fn drop(&mut self) {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|(n, _)| *n != self.id);
    }
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

/// The connection's reader once its first line, given within [`SECRET_WAIT`], is the secret, and
/// from then on waiting for each line as long as it takes; `None` when it is not. The wait is set
/// and lifted on the copy of the connection that reads: on Windows copies do not share it.
fn sign_in(stream: &TcpStream, secret: &str) -> Option<BufReader<TcpStream>> {
    let read_half = stream.try_clone().ok()?;
    read_half.set_nonblocking(false).ok()?;
    read_half.set_read_timeout(Some(SECRET_WAIT)).ok()?;
    let mut reader = BufReader::new(read_half);
    let given = read_line(&mut reader).unwrap_or_default();
    if !same_secret(given.as_bytes(), secret.as_bytes()) {
        return None;
    }
    reader.get_ref().set_read_timeout(None).ok()?;
    Some(reader)
}

impl Connection {
    /// The secret, then lines both ways until either side ends.
    fn serve(self, stream: TcpStream) {
        let _ = stream.set_nonblocking(false);
        let Some(mut reader) = sign_in(&stream, &self.secret) else {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return;
        };
        let (Ok(write_half), Ok(connection)) = (stream.try_clone(), stream.try_clone()) else {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return;
        };
        let _ = write_half.set_nonblocking(false);
        let (tx, rx) = mpsc::sync_channel::<String>(QUEUE);
        let backlog: Vec<String> = {
            let mut out = self.output.lock().unwrap_or_else(PoisonError::into_inner);
            out.readers.push(Reader {
                lines: tx,
                connection,
            });
            out.backlog.iter().cloned().collect()
        };
        let ended = Arc::new(AtomicBool::new(false));
        let writer = {
            let ended = Arc::clone(&ended);
            let hello = self.hello;
            std::thread::Builder::new()
                .name("Admin channel writer".into())
                .spawn(move || write_lines(write_half, &hello, backlog, &rx, &ended))
        };
        // Without its writer the program would wait for a hello that never comes: it is closed.
        let Ok(writer) = writer else {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            return;
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

#[cfg(test)]
mod tests {
    //! Divergence: V456 (the admin channel is this server's own; ACE has none).
    //! The secret's wait, a connection's place, and a program that falls behind the console.
    use super::*;

    /// A program that has given the secret is waited for however long it sends nothing: the
    /// secret's wait is lifted on the connection its lines are read from.
    #[test]
    fn a_signed_in_program_is_waited_for_however_long_it_is_idle() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a listener");
        let mut program =
            TcpStream::connect(listener.local_addr().expect("its address")).expect("connect");
        let (server, _) = listener.accept().expect("accepted");
        writeln!(program, "the secret").expect("written");
        let reader = sign_in(&server, "the secret").expect("signed in");
        assert_eq!(
            reader.get_ref().read_timeout().expect("its wait"),
            None,
            "no wait on the connection it reads"
        );
    }

    /// A connection's place is given back however its thread ends: a thread never started, or
    /// one that fails.
    #[test]
    fn a_connections_place_is_given_back_however_its_thread_ends() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a listener");
        let open: Open = Arc::new(Mutex::new(Vec::new()));
        for id in [1, 2] {
            let s =
                TcpStream::connect(listener.local_addr().expect("its address")).expect("connect");
            open.lock().expect("the list").push((id, s));
        }
        let place = |id| Place {
            open: Arc::clone(&open),
            id,
        };
        drop(place(1));
        let failed = place(2);
        let ended = std::thread::spawn(move || {
            let _place = failed;
            panic!("the connection's thread fails");
        })
        .join();
        assert!(ended.is_err(), "the thread failed");
        assert!(
            open.lock().expect("the list").is_empty(),
            "both places back"
        );
    }

    /// A program too far behind the console's lines is closed, and its place is freed for another.
    #[test]
    fn a_program_too_far_behind_the_console_is_closed_and_its_place_freed() {
        let dir = std::env::temp_dir().join(format!(
            "empyrean-admin-channel-behind-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        let (commands, _world) = mpsc::channel();
        let channel = start(&dir, commands, serde_json::json!({ "hello": "empyrean" }))
            .expect("the channel starts");
        let secret = read_file(&dir).expect("its file").secret;
        let let_in = || {
            let mut s = TcpStream::connect(channel.local_addr()).expect("connect");
            s.set_read_timeout(Some(Duration::from_secs(60)))
                .expect("a guard against a hang");
            writeln!(s, "{secret}").expect("the secret");
            let mut reader = BufReader::new(s.try_clone().expect("a reader"));
            let mut hello = String::new();
            reader.read_line(&mut hello).expect("its hello");
            assert!(hello.contains("empyrean"), "{hello:?}");
            (s, reader)
        };
        let readers = || {
            channel
                .output
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .readers
                .len()
        };
        let mut connected = Vec::new();
        for _ in 0..MAX_CLIENTS {
            connected.push(let_in());
        }
        // Each is a reader once its connection has signed in; the count only stops a hang.
        let mut count = 0;
        while readers() < MAX_CLIENTS {
            std::thread::yield_now();
            count += 1;
            assert!(count < 100_000_000, "not every program became a reader");
        }
        // None of the four reads, so the console's lines pile up until each is too far behind;
        // the count only stops a hang.
        let line = "x".repeat(1000);
        let mut count = 0;
        while readers() > 0 {
            channel
                .output
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(&line);
            count += 1;
            assert!(count < 10_000_000, "no program fell behind");
        }
        // Each reads what reached it, then the end: the channel closed it. On Windows a connection
        // closed with lines still unread on its side ends in a reset instead, which is closed too;
        // the read's guard running out is not.
        for (_s, mut reader) in connected {
            let mut rest = Vec::new();
            let end = reader.read_to_end(&mut rest);
            let closed = match &end {
                Ok(_) => true,
                Err(e) => matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
                ),
            };
            assert!(closed, "{end:?} after {} bytes", rest.len());
        }
        // Their places are free once their threads end: another program is let in. The count
        // only stops a hang.
        let mut tries = 0;
        loop {
            let open = channel
                .open
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .len();
            if open == 0 {
                break;
            }
            std::thread::yield_now();
            tries += 1;
            assert!(tries < 100_000_000, "{open} places still held");
        }
        drop(let_in());
        channel.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
