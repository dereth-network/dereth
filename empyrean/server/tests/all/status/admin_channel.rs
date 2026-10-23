//! Vectors: local console lines, loopback connections and a world thread of the test's own.
//! The admin channel: its file in the server's folder (loopback address, a new secret, the
//! process), the secret first or nothing, the console's lines to whoever holds it and its commands
//! to the world thread, at most four connected, and the file gone once it stops.
//! Fixture: a temporary folder, a world with fake dat files on a thread of its own, and the
//! process's console, one channel at a time.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;

use empyrean_command::command_manager::{self, WorldCommand};
use empyrean_command::console;
use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_dat::FakeDats;
use empyrean_server::admin_channel::{self, AdminChannel, MAX_CLIENTS};
use empyrean_world::World;

/// The console and its tap are the process's: one channel at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// A world on a thread of its own, running what it is sent, as the server's world thread does.
struct WorldThread {
    tx: Option<Sender<WorldCommand>>,
    thread: Option<JoinHandle<()>>,
}

impl WorldThread {
    fn new() -> Self {
        command_manager::initialize(None);
        let (tx, rx) = mpsc::channel::<WorldCommand>();
        let thread = std::thread::spawn(move || {
            let clock = VirtualClock::default();
            let mut w = World::new(
                ClockSnapshot::take(&clock, 0.0),
                FakeDats::new().build().expect("empty fake dats"),
            );
            for command in rx {
                command(&mut w);
            }
        });
        Self {
            tx: Some(tx),
            thread: Some(thread),
        }
    }

    fn commands(&self) -> Sender<WorldCommand> {
        self.tx.clone().expect("the world thread's queue")
    }
}

impl Drop for WorldThread {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A folder of the test's own, emptied.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    dir
}

fn start(dir: &Path, world: &WorldThread) -> AdminChannel {
    admin_channel::start(
        dir,
        world.commands(),
        serde_json::json!({ "hello": "empyrean", "version": "9.9.9", "world": "Test" }),
    )
    .expect("the channel starts")
}

/// Connects, with a guard against a hang only.
fn connect(channel: &AdminChannel) -> TcpStream {
    let s = TcpStream::connect(channel.local_addr()).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(60)))
        .expect("a guard");
    s
}

/// Connects and gives the secret.
fn signed_in(channel: &AdminChannel, dir: &Path) -> (TcpStream, BufReader<TcpStream>) {
    let mut s = connect(channel);
    let file = admin_channel::read_file(dir).expect("the channel's file");
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

/// Lines until one whose `line` contains `text`, which is returned.
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
    matches!(s.read_to_end(&mut got), Ok(0)) || (got.is_empty())
}

#[test]
fn the_channel_writes_where_it_listens_and_a_new_secret_in_the_servers_folder_and_removes_them() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let world = WorldThread::new();
    let dir = scratch("admin-channel-file");
    let channel = start(&dir, &world);
    let file = admin_channel::read_file(&dir).expect("the channel's file");
    assert_eq!(channel.file(), dir.join(admin_channel::FILE_NAME));
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
        admin_channel::read_file(&dir).is_none(),
        "the file goes when the channel stops"
    );
    let again = start(&dir, &world);
    let second = admin_channel::read_file(&dir).expect("the file again");
    assert_ne!(second.secret, file.secret, "a new secret at each start");
    again.shutdown();
}

#[test]
fn a_connection_that_does_not_begin_with_the_secret_is_closed_without_a_word() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let world = WorldThread::new();
    let dir = scratch("admin-channel-secret");
    let channel = start(&dir, &world);
    let secret = admin_channel::read_file(&dir).expect("the file").secret;
    for first in [
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n".to_owned(),
        format!("{}0\n", secret),
        format!("{}\n", &secret[..63]),
        "\n".to_owned(),
    ] {
        let mut s = connect(&channel);
        s.write_all(first.as_bytes()).expect("write");
        assert!(closed_without_a_word(s), "{first:?}");
    }
    channel.shutdown();
}

#[test]
fn with_the_secret_a_program_reads_the_console_and_its_commands_reach_the_world() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let world = WorldThread::new();
    let dir = scratch("admin-channel-console");
    let channel = start(&dir, &world);
    console::write_line("written before the program connected");
    let (mut s, mut reader) = signed_in(&channel, &dir);
    let hello = next(&mut reader);
    assert_eq!(hello["hello"], "empyrean");
    assert_eq!(hello["version"], "9.9.9");
    assert_eq!(hello["world"], "Test");
    until(&mut reader, "written before the program connected");
    console::write_line("written while it is connected");
    until(&mut reader, "written while it is connected");
    // A command typed on the channel runs on the world thread, as one typed at the console, and
    // its reply comes back over the channel.
    writeln!(s, "source").expect("a command");
    until(&mut reader, "free software");
    writeln!(s, "no-such-command").expect("a command");
    until(&mut reader, "Invalid Command");
    drop(s);
    channel.shutdown();
}

#[test]
fn at_most_four_programs_are_connected_at_once() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let world = WorldThread::new();
    let dir = scratch("admin-channel-four");
    let channel = start(&dir, &world);
    let mut connected = Vec::new();
    for _ in 0..MAX_CLIENTS {
        let (s, mut reader) = signed_in(&channel, &dir);
        assert_eq!(next(&mut reader)["hello"], "empyrean");
        connected.push((s, reader));
    }
    let (extra, _) = signed_in(&channel, &dir);
    assert!(closed_without_a_word(extra), "a fifth is turned away");
    drop(connected.pop());
    // The one that left frees its place once its connection has ended; the guard only stops a
    // hang.
    let guard = std::time::Instant::now();
    loop {
        let (s, mut reader) = signed_in(&channel, &dir);
        let mut line = String::new();
        if reader.read_line(&mut line).is_ok_and(|n| n > 0) {
            assert!(line.contains("empyrean"), "{line}");
            drop(s);
            break;
        }
        assert!(guard.elapsed() < Duration::from_secs(120), "no place freed");
        std::thread::sleep(Duration::from_millis(20));
    }
    channel.shutdown();
}
