//! The library's sockets: each connection on threads of its own, so that nothing the client calls
//! ever waits on the network.
//!
//! Naming the world's host and opening a connection both block, and the client's thread must never
//! wait, so a connection is dialed on a thread of its own, which then writes what it is given and
//! hands the reading to a second thread. Both report through one channel, which the library reads
//! inside the client's pump. Closing a connection drops its writer's channel: the writer writes
//! what it was given before, shuts the socket, and the reader ends. The threads hold nothing of the
//! client's and keep nothing alive: the process ends them when it ends.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

/// How long one address may take to answer a dial.
pub const DIAL_TIMEOUT: Duration = Duration::from_secs(5);

/// What a connection's threads report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetEvent {
    /// The connection is open.
    Opened(u64),
    /// The connection could not be opened.
    Failed(u64),
    /// Bytes read from the connection.
    Received(u64, Vec<u8>),
    /// The connection ended.
    Closed(u64),
}

/// The library's connections.
#[derive(Debug)]
pub struct Net {
    events: Sender<NetEvent>,
    inbox: Receiver<NetEvent>,
    writers: BTreeMap<u64, Sender<Vec<u8>>>,
}

impl Default for Net {
    fn default() -> Self {
        Self::new()
    }
}

impl Net {
    /// No connections.
    #[must_use]
    pub fn new() -> Self {
        let (events, inbox) = mpsc::channel();
        Self {
            events,
            inbox,
            writers: BTreeMap::new(),
        }
    }

    /// Open connection `conn` to `host` and `port`, on threads of its own.
    pub fn dial(&mut self, conn: u64, host: &str, port: u16) {
        let (tx, rx) = mpsc::channel();
        self.writers.insert(conn, tx);
        let events = self.events.clone();
        let host = host.to_owned();
        let spawned = thread::Builder::new()
            .name("friends-connection".into())
            .spawn(move || connection(conn, &host, port, &rx, &events));
        if spawned.is_err() {
            self.writers.remove(&conn);
            let _ = self.events.send(NetEvent::Failed(conn));
        }
    }

    /// Write to connection `conn`, after what was given before.
    pub fn write(&mut self, conn: u64, bytes: Vec<u8>) {
        if let Some(tx) = self.writers.get(&conn) {
            let _ = tx.send(bytes);
        }
    }

    /// Close connection `conn` once what was given before is written.
    pub fn close(&mut self, conn: u64) {
        self.writers.remove(&conn);
    }

    /// What the connections reported since the last call, in order.
    pub fn events(&mut self) -> Vec<NetEvent> {
        self.inbox.try_iter().collect()
    }
}

/// One connection's life: dial, then write what is given until the channel closes.
fn connection(
    conn: u64,
    host: &str,
    port: u16,
    writes: &Receiver<Vec<u8>>,
    events: &Sender<NetEvent>,
) {
    let Some(mut stream) = dial(host, port) else {
        let _ = events.send(NetEvent::Failed(conn));
        return;
    };
    let _ = stream.set_nodelay(true);
    let Ok(reader) = stream.try_clone() else {
        let _ = events.send(NetEvent::Failed(conn));
        return;
    };
    let _ = events.send(NetEvent::Opened(conn));
    let to = events.clone();
    let spawned = thread::Builder::new()
        .name("friends-reader".into())
        .spawn(move || read(conn, reader, &to));
    if spawned.is_err() {
        let _ = events.send(NetEvent::Closed(conn));
        let _ = stream.shutdown(Shutdown::Both);
        return;
    }
    while let Ok(bytes) = writes.recv() {
        if stream.write_all(&bytes).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

/// Each of the host's addresses in turn, until one answers.
fn dial(host: &str, port: u16) -> Option<TcpStream> {
    (host, port)
        .to_socket_addrs()
        .ok()?
        .find_map(|a| TcpStream::connect_timeout(&a, DIAL_TIMEOUT).ok())
}

/// Read until the connection ends, handing on what comes.
fn read(conn: u64, mut stream: TcpStream, events: &Sender<NetEvent>) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        match stream.read(&mut buf) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Ok(0) | Err(_) => {
                let _ = events.send(NetEvent::Closed(conn));
                return;
            }
            Ok(n) => {
                if events
                    .send(NetEvent::Received(conn, buf[..n].to_vec()))
                    .is_err()
                {
                    return;
                }
            }
        }
    }
}
