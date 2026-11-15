//! The admin channel: the server's console for a program on the same computer, in the form of
//! Empyrean's.
//!
//! A world that runs as a service has no one at its console. With `[server] admin_channel` on
//! (`--admin-channel`; off by default), the server listens on loopback, on a port the system picks,
//! and writes where and with what secret to `admin-channel.json` in its own folder (the
//! configuration file's, else the working folder): `{"address", "secret", "pid"}`. A program that
//! can read that file can then type console commands and read what the console shows, as a person
//! at the console would. The file is removed when the server stops, and a file left by a server
//! that did not stop cleanly names a process that is no longer there.
//!
//! **Who may use it.** Whoever can read the world's own folder, and no one else: the secret is 32
//! random bytes drawn at each start, kept nowhere but in that file, which on Linux and macOS is
//! readable by the server's own user alone; on Windows it has the folder's permissions. The
//! listener takes connections from loopback only. A connection's first line must be the secret,
//! within [`SECRET_WAIT`]; anything else closes it without a word, so a web page that makes a
//! browser connect to the port learns nothing. At most [`MAX_CLIENTS`] are connected at once.
//!
//! **The conversation.** Lines of UTF-8 both ways. After the secret, each line the program sends
//! is a console command, run as one typed at the console is; a line over [`MAX_LINE`] bytes closes
//! the connection. The server sends JSON, one object a line: first `{"hello": "yalain", "version",
//! "world"}`, then the console's last [`BACKLOG`] lines, then each line the console shows as it is
//! shown, `{"line": "..."}`: answers and log lines alike. A program that does not read its lines
//! quickly enough is dropped rather than let the console wait for it.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

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

/// The running channel's output, which the console's lines are handed to.
static TAP: Mutex<Option<Arc<Mutex<Output>>>> = Mutex::new(None);

/// A line the console shows: handed to the channel's programs while it runs.
pub fn shown(line: &str) {
    let tap = TAP.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if let Some(output) = tap {
        output
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }
}

/// Where the log goes: standard output, as before, and each of its lines to the channel's
/// programs while it runs ([`shown`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct Console;

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Console {
    type Writer = ConsoleLines;

    fn make_writer(&'a self) -> Self::Writer {
        ConsoleLines(Vec::new())
    }
}

/// One log event's text, written out as it is let go.
#[derive(Debug)]
pub struct ConsoleLines(Vec<u8>);

impl Write for ConsoleLines {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for ConsoleLines {
    fn drop(&mut self) {
        let _ = std::io::stdout().lock().write_all(&self.0);
        for line in String::from_utf8_lossy(&self.0).lines() {
            shown(line);
        }
    }
}

/// The open connections, each by its number, so the channel can end them and count them.
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
        *TAP.lock().unwrap_or_else(PoisonError::into_inner) = None;
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
/// file, and from then on hands each line a program sends to the console through `lines`. `hello`
/// is what a program is told first (the server's version and world name).
///
/// # Errors
/// When the port cannot be bound or the file cannot be written; nothing is left listening then.
pub fn start(
    folder: &Path,
    lines: Sender<String>,
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
    *TAP.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(&output));
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
                    let Ok((stream, peer)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
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
                        lines: lines.clone(),
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
    lines: Sender<String>,
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
        while let Some(line) = read_line(&mut reader) {
            if self.lines.send(line).is_err() {
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
#[cfg(test)]
#[must_use]
pub fn read_file(folder: &Path) -> Option<ChannelFile> {
    let mut text = String::new();
    std::fs::File::open(folder.join(FILE_NAME))
        .ok()?
        .read_to_string(&mut text)
        .ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The channel's output is the process's: one channel at a time.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// A folder of the test's own, emptied.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "yalain-admin-channel-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a folder");
        dir
    }

    fn start_in(dir: &Path) -> (AdminChannel, Receiver<String>) {
        let (tx, rx) = mpsc::channel();
        let channel = start(
            dir,
            tx,
            serde_json::json!({ "hello": "yalain", "version": "9.9.9", "world": "Test" }),
        )
        .expect("the channel starts");
        (channel, rx)
    }

    /// Connects, with a guard against a hang only.
    fn connect(channel: &AdminChannel) -> TcpStream {
        let s = TcpStream::connect(channel.local_addr()).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(60)))
            .expect("a guard");
        s
    }

    /// Signs in until the channel lets the program in, and reads its hello. A connection turned
    /// away while the channel is full is closed; each try waits for the channel's answer, a hello
    /// or a close, and the count only stops a hang.
    fn let_in(channel: &AdminChannel, dir: &Path) -> (TcpStream, BufReader<TcpStream>) {
        let mut tries = 0;
        loop {
            let (s, mut reader) = signed_in(channel, dir);
            let mut line = String::new();
            if reader.read_line(&mut line).is_ok_and(|n| n > 0) {
                let hello: serde_json::Value =
                    serde_json::from_str(&line).unwrap_or_else(|_| panic!("JSON: {line:?}"));
                assert_eq!(hello["hello"], "yalain", "{line}");
                return (s, reader);
            }
            tries += 1;
            assert!(tries < 100_000, "no place freed");
        }
    }

    /// Connects and gives the secret.
    fn signed_in(channel: &AdminChannel, dir: &Path) -> (TcpStream, BufReader<TcpStream>) {
        let mut s = connect(channel);
        let file = read_file(dir).expect("the channel's file");
        // A connection turned away may already be closed; what it reads says so.
        let _ = writeln!(s, "{}", file.secret);
        let reader = BufReader::new(s.try_clone().expect("a reader"));
        (s, reader)
    }

    /// The next line the channel sends, as JSON.
    fn next(reader: &mut BufReader<TcpStream>) -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).expect("a line");
        serde_json::from_str(&line).unwrap_or_else(|_| panic!("JSON: {line:?}"))
    }

    /// Lines until one whose `line` holds `text`, which is returned.
    fn until(reader: &mut BufReader<TcpStream>, text: &str) -> String {
        loop {
            let v = next(reader);
            if let Some(line) = v["line"].as_str() {
                if line.contains(text) {
                    return line.to_owned();
                }
            }
        }
    }

    /// Whether the channel closed `s` without sending anything.
    fn closed_without_a_word(mut s: TcpStream) -> bool {
        let mut got = Vec::new();
        matches!(s.read_to_end(&mut got), Ok(0)) || got.is_empty()
    }

    /// The channel writes its loopback address, a new secret of 64 hexadecimal digits and the
    /// server's process to `admin-channel.json` in the server's folder, readable by its own user
    /// alone, and removes the file when it stops; each start draws a new secret.
    /// Behaviour: server.operator.admin-channel-file
    #[test]
    fn the_channel_writes_where_it_listens_and_a_new_secret_in_the_servers_folder_and_removes_them()
    {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
        let dir = scratch("file");
        let (channel, _lines) = start_in(&dir);
        let file = read_file(&dir).expect("the channel's file");
        assert_eq!(channel.file(), dir.join(FILE_NAME));
        assert_eq!(file.address, channel.local_addr().to_string());
        assert!(channel.local_addr().ip().is_loopback(), "{}", file.address);
        assert_eq!(file.pid, std::process::id());
        assert_eq!(file.secret.len(), 64);
        assert!(file.secret.bytes().all(|b| b.is_ascii_hexdigit()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(channel.file())
                .expect("its file")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "readable by the server's user alone");
        }
        channel.shutdown();
        assert!(
            read_file(&dir).is_none(),
            "the file goes when the channel stops"
        );
        let (again, _lines) = start_in(&dir);
        let second = read_file(&dir).expect("the file again");
        assert_ne!(second.secret, file.secret, "a new secret at each start");
        again.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A connection whose first line is not the secret (a web request, the secret with more, less
    /// of it, an empty line) is closed without a word; with it, at most four programs are connected
    /// at once, and a fifth is turned away until one leaves.
    /// Behaviour: server.operator.admin-channel-secret-first
    #[test]
    fn only_the_secret_opens_the_channel_and_to_at_most_four_programs_at_once() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
        let dir = scratch("secret");
        let (channel, _lines) = start_in(&dir);
        let secret = read_file(&dir).expect("the file").secret;
        for first in [
            "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n".to_owned(),
            format!("{secret}0\n"),
            format!("{}\n", &secret[..63]),
            "\n".to_owned(),
        ] {
            let mut s = connect(&channel);
            s.write_all(first.as_bytes()).expect("write");
            assert!(closed_without_a_word(s), "{first:?}");
        }
        // A connection turned away keeps its place until its own thread ends, a moment after its
        // close, so each of the four is let in once a place is free.
        let mut connected = Vec::new();
        for _ in 0..MAX_CLIENTS {
            connected.push(let_in(&channel, &dir));
        }
        let (extra, _) = signed_in(&channel, &dir);
        assert!(closed_without_a_word(extra), "a fifth is turned away");
        drop(connected.pop());
        // The one that left frees its place once its connection has ended.
        drop(let_in(&channel, &dir));
        drop(connected);
        channel.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With the secret, a program is told the hello, then the console's lines from before it
    /// connected and each as it is shown, the log's lines among them; each line it sends reaches
    /// the console, and a line too long closes it.
    /// Behaviour: server.operator.admin-channel-console
    #[test]
    fn with_the_secret_a_program_reads_the_console_and_its_lines_reach_the_console() {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
        let dir = scratch("console");
        let (channel, lines) = start_in(&dir);
        shown("shown before the program connected");
        let (mut s, mut reader) = signed_in(&channel, &dir);
        let hello = next(&mut reader);
        assert_eq!(hello["hello"], "yalain");
        assert_eq!(hello["version"], "9.9.9");
        assert_eq!(hello["world"], "Test");
        until(&mut reader, "shown before the program connected");
        // The log's own writer hands its lines on as it writes them out.
        {
            use tracing_subscriber::fmt::MakeWriter as _;
            let mut w = Console.make_writer();
            w.write_all(b"2026-11-10 INFO a log line\n")
                .expect("written");
        }
        until(&mut reader, "a log line");
        writeln!(s, "help").expect("a command");
        writeln!(s, "accountlist").expect("a command");
        let got: Vec<String> = (0..2)
            .map(|_| lines.recv_timeout(Duration::from_secs(60)).expect("a line"))
            .collect();
        assert_eq!(got, ["help", "accountlist"]);
        s.write_all(&vec![b'x'; MAX_LINE + 10])
            .expect("a long line");
        // The channel ends the connection; what it sent before is all there is.
        let mut rest = Vec::new();
        let _ = reader.read_to_end(&mut rest);
        assert!(lines.try_recv().is_err(), "not a command");
        channel.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
