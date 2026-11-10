//! Not ACE (V519): the patch as one TCP stream, for a client that asks for it.
//!
//! A client patched to a world with a data overlay that sets `FLAG_BULK` in its interrogation
//! response is sent `0xF7ED` after `0xF7E7`, in place of the patch's `0xF7E2`s: a port and a
//! single-use token for a ticket that holds the records it lacks. It connects over TCP from the
//! session's own address, sends the magic, the version and the token, and reads every record as
//! a `DddData` body behind its length as a `u32`, until a length of 0. TCP's own flow control is
//! the only limit on the rate; the records go in whatever order the workers that build them
//! finish. A ticket no client claims in [`TICKET_SECONDS`], and whatever a client that could not
//! finish names in `0xF7EE`, goes back to the rate-limited `0xF7E2` queue, so a client that cannot
//! reach the port is still patched.
//!
//! The listener holds at most [`Limits::streams`] connections at once, each with its own thread
//! and [`Limits::workers`] more while it sends; a connection past the cap, or from an address
//! that holds no ticket, is closed before anything is read, and its client falls back to `0xF7E2`.

use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use dereth_protocol::admin::{BULK_MAGIC, BULK_VERSION};
use empyrean_dat::{DatDatabaseType, DatManager};

use crate::managers::ddd_manager;
use crate::network::game_messages::messages::game_message_ddd_data_message::ddd_data_body;
use crate::World;
use empyrean_net::SessionId;

/// How long a ticket waits for its client to connect before its records go back to `0xF7E2`.
pub const TICKET_SECONDS: f64 = 30.0;
/// A ticket still unclaimed this long after it was made is dropped, whatever became of its
/// session.
const STALE: Duration = Duration::from_secs(600);
/// The zlib level the stream deflates a compressed record at: fast, as the stream is.
const LEVEL: i32 = 6;

/// The listener's bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Connections held at once, each a thread; one past it is closed at once.
    pub streams: usize,
    /// The threads that build one stream's records.
    pub workers: usize,
    /// How long a connection may take to send its hello.
    pub hello: Duration,
    /// How long one write to a client may wait.
    pub write: Duration,
    /// Whether a ticket opens only from its session's own address (off only in tests, whose
    /// sessions have no real address).
    pub same_address: bool,
}

/// The default [`Limits::streams`].
pub const DEFAULT_STREAMS: usize = 16;

impl Default for Limits {
    fn default() -> Self {
        Self {
            streams: DEFAULT_STREAMS,
            workers: std::thread::available_parallelism()
                .map_or(2, std::num::NonZero::get)
                .clamp(1, 4),
            hello: Duration::from_secs(10),
            write: Duration::from_secs(60),
            same_address: true,
        }
    }
}

/// One record of a ticket: the file, its dat, and whether it goes compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BulkItem {
    pub id: u32,
    pub db: DatDatabaseType,
    pub compress: bool,
}

/// A session's offered stream: its ticket's token, when it goes back to `0xF7E2` unless claimed,
/// whether a client claimed it, and the records not yet sent the old way, for that fallback.
#[derive(Debug, Clone)]
pub struct BulkPending {
    pub token: [u8; 16],
    pub deadline: empyrean_common::dotnet::DotNetDateTime,
    pub claimed: bool,
    pub items: Vec<(u32, DatDatabaseType)>,
}

struct Ticket {
    items: Vec<BulkItem>,
    dats: Arc<DatManager>,
    account: String,
    address: IpAddr,
    made: Instant,
}

/// The stream's listener: its tickets and what it is holding.
pub struct Service {
    port: u16,
    limits: Limits,
    tickets: Mutex<HashMap<[u8; 16], Ticket>>,
    /// Connections held now, and at most so far.
    held: AtomicUsize,
    peak: AtomicUsize,
    /// The listener's threads alive now, beyond its accept loop: one per connection and the
    /// workers of each stream.
    threads: AtomicUsize,
    /// Connections closed at once for the cap.
    turned_away: AtomicUsize,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("port", &self.port)
            .field("limits", &self.limits)
            .field("held", &self.held)
            .finish_non_exhaustive()
    }
}

/// Counts one of a [`Service`]'s threads (or connections) while it lives.
struct Counted<'a>(&'a AtomicUsize);

impl<'a> Counted<'a> {
    fn new(n: &'a AtomicUsize) -> Self {
        n.fetch_add(1, Ordering::SeqCst);
        Self(n)
    }
}

impl Drop for Counted<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// An address as the listener compares it: an IPv4 address mapped into IPv6 is that IPv4 one.
fn canonical(ip: IpAddr) -> IpAddr {
    ip.to_canonical()
}

impl Service {
    /// Listen on `addr` within `limits`.
    ///
    /// # Errors
    /// The address cannot be bound, or the thread cannot start.
    pub fn listen(addr: SocketAddr, limits: Limits) -> std::io::Result<Arc<Self>> {
        let listener = TcpListener::bind(addr)?;
        let service = Arc::new(Self {
            port: listener.local_addr()?.port(),
            limits,
            tickets: Mutex::new(HashMap::new()),
            held: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            threads: AtomicUsize::new(0),
            turned_away: AtomicUsize::new(0),
        });
        let theirs = Arc::clone(&service);
        std::thread::Builder::new()
            .name("Bulk Patch".to_owned())
            .spawn(move || {
                for conn in listener.incoming() {
                    let Ok(stream) = conn else { continue };
                    Arc::clone(&theirs).accept(stream);
                }
            })?;
        Ok(service)
    }

    /// The port it listens on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Its bounds.
    #[must_use]
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Connections held now.
    #[must_use]
    pub fn held(&self) -> usize {
        self.held.load(Ordering::SeqCst)
    }

    /// The most connections held at once so far.
    #[must_use]
    pub fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    /// Its threads alive now, beyond the accept loop.
    #[must_use]
    pub fn threads(&self) -> usize {
        self.threads.load(Ordering::SeqCst)
    }

    /// Connections closed at once because it held as many as it may.
    #[must_use]
    pub fn turned_away(&self) -> usize {
        self.turned_away.load(Ordering::SeqCst)
    }

    /// Whether some ticket opens from `ip`.
    fn expects(&self, ip: IpAddr) -> bool {
        !self.limits.same_address
            || self
                .tickets
                .lock()
                .is_ok_and(|t| t.values().any(|t| t.address == canonical(ip)))
    }

    fn accept(self: Arc<Self>, stream: TcpStream) {
        let Ok(peer) = stream.peer_addr() else {
            return;
        };
        if !self.expects(peer.ip()) {
            log::debug!("[DDD] bulk patch: {peer} holds no ticket; closed");
            return;
        }
        // Taken before the thread starts, so the cap holds however fast connections come.
        let held = self.held.fetch_add(1, Ordering::SeqCst) + 1;
        if held > self.limits.streams {
            self.held.fetch_sub(1, Ordering::SeqCst);
            self.turned_away.fetch_add(1, Ordering::SeqCst);
            log::info!(
                "[DDD] bulk patch: {peer} turned away, {} streams already open; it is patched over UDP",
                self.limits.streams
            );
            return;
        }
        self.peak.fetch_max(held, Ordering::SeqCst);
        self.threads.fetch_add(1, Ordering::SeqCst);
        let me = Arc::clone(&self);
        let spawned = std::thread::Builder::new()
            .name("Bulk Patch Stream".to_owned())
            .spawn(move || {
                if let Err(e) = me.serve(stream, peer.ip()) {
                    log::info!("[DDD] bulk patch stream to {peer} ended: {e}");
                }
                me.threads.fetch_sub(1, Ordering::SeqCst);
                me.held.fetch_sub(1, Ordering::SeqCst);
            });
        if spawned.is_err() {
            self.threads.fetch_sub(1, Ordering::SeqCst);
            self.held.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// A ticket for `items` from `dats`, for `account`, which opens from `address` alone: its
    /// token.
    #[must_use]
    pub fn offer(
        &self,
        items: Vec<BulkItem>,
        dats: Arc<DatManager>,
        account: &str,
        address: IpAddr,
    ) -> Option<[u8; 16]> {
        let mut token = [0u8; 16];
        getrandom::fill(&mut token).ok()?;
        let mut tickets = self.tickets.lock().ok()?;
        tickets.retain(|_, t| t.made.elapsed() < STALE);
        tickets.insert(
            token,
            Ticket {
                items,
                dats,
                account: account.to_owned(),
                address: canonical(address),
                made: Instant::now(),
            },
        );
        Some(token)
    }

    /// Take back a ticket no client has claimed; false when one has, or it is gone.
    pub fn withdraw(&self, token: &[u8; 16]) -> bool {
        self.tickets
            .lock()
            .is_ok_and(|mut t| t.remove(token).is_some())
    }

    /// The ticket `token` opens from `ip`, taken so that it opens nothing again. A token presented
    /// from another address opens nothing and is left for its own client.
    fn claim(&self, token: &[u8; 16], ip: IpAddr) -> Option<Ticket> {
        let mut tickets = self.tickets.lock().ok()?;
        let from_its_own = tickets
            .get(token)
            .is_some_and(|t| !self.limits.same_address || t.address == canonical(ip));
        if from_its_own {
            tickets.remove(token)
        } else {
            None
        }
    }

    fn serve(&self, mut stream: TcpStream, ip: IpAddr) -> std::io::Result<()> {
        use std::io::{Error, ErrorKind};
        stream.set_read_timeout(Some(self.limits.hello))?;
        let mut hello = [0u8; 24];
        stream.read_exact(&mut hello)?;
        let version = u32::from_le_bytes([hello[4], hello[5], hello[6], hello[7]]);
        if hello[..4] != BULK_MAGIC || version != BULK_VERSION {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "not a bulk patch request",
            ));
        }
        let mut token = [0u8; 16];
        token.copy_from_slice(&hello[8..]);
        let ticket = self
            .claim(&token, ip)
            .ok_or_else(|| Error::new(ErrorKind::PermissionDenied, "no such ticket"))?;
        stream.set_write_timeout(Some(self.limits.write))?;
        let started = Instant::now();
        let (records, bytes) = self.send(stream, &ticket)?;
        let secs = started.elapsed().as_secs_f64();
        #[allow(clippy::cast_precision_loss)] // Megabytes for a log line.
        let mib = bytes as f64 / 1_048_576.0;
        log::info!(
            "[DDD] client {} took the patch as a stream: {records} records, {mib:.1} MiB in {secs:.1} s ({:.1} MiB/s)",
            ticket.account,
            mib / secs.max(1e-3)
        );
        Ok(())
    }

    /// Every record of `ticket` down `stream`, in the ticket's order, built on
    /// [`Limits::workers`] threads, then a length of 0. A worker builds a record at most
    /// [`AHEAD`] places past the last one written, so what waits to be written is bounded.
    fn send(&self, stream: TcpStream, ticket: &Ticket) -> std::io::Result<(usize, u64)> {
        let next = AtomicUsize::new(0);
        // The records written so far, and whether the writer has stopped.
        let written = (Mutex::new((0usize, false)), Condvar::new());
        let (tx, rx) = std::sync::mpsc::sync_channel::<(usize, Option<Vec<u8>>)>(AHEAD);
        std::thread::scope(|s| {
            for _ in 0..self.limits.workers.max(1) {
                let tx = tx.clone();
                let (next, written) = (&next, &written);
                let threads = &self.threads;
                s.spawn(move || {
                    let _alive = Counted::new(threads);
                    loop {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = ticket.items.get(at) else {
                            break;
                        };
                        {
                            let Ok(mut w) = written.0.lock() else { break };
                            while !w.1 && at >= w.0 + AHEAD {
                                w = match written.1.wait(w) {
                                    Ok(w) => w,
                                    Err(_) => return,
                                };
                            }
                            if w.1 {
                                break;
                            }
                        }
                        if tx.send((at, frame(ticket, item))).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            let stop = || {
                if let Ok(mut w) = written.0.lock() {
                    w.1 = true;
                }
                written.1.notify_all();
            };
            let mut out = BufWriter::with_capacity(1 << 20, stream);
            let (mut records, mut bytes) = (0usize, 0u64);
            let mut waiting: std::collections::BTreeMap<usize, Option<Vec<u8>>> =
                std::collections::BTreeMap::new();
            let mut due = 0usize;
            for (at, frame) in rx {
                waiting.insert(at, frame);
                while let Some(frame) = waiting.remove(&due) {
                    if let Some(frame) = frame {
                        if let Err(e) = out.write_all(&frame) {
                            stop();
                            return Err(e);
                        }
                        records += 1;
                        bytes += frame.len() as u64;
                    }
                    due += 1;
                    if let Ok(mut w) = written.0.lock() {
                        w.0 = due;
                    }
                    written.1.notify_all();
                }
            }
            let end = out
                .write_all(&0u32.to_le_bytes())
                .and_then(|()| out.flush());
            stop();
            end.map(|()| (records, bytes))
        })
    }
}

/// How far past the last record written a worker may build one.
const AHEAD: usize = 64;

/// `item` of `ticket` behind its length, as the stream sends it, or `None` when it cannot be read.
fn frame(ticket: &Ticket, item: &BulkItem) -> Option<Vec<u8>> {
    let level = item.compress.then_some(LEVEL);
    let Some(contents) = ddd_manager::dat_file_contents(&ticket.dats, item.id, item.db, level)
    else {
        log::warn!("[DDD] bulk patch: 0x{:08X} could not be read", item.id);
        return None;
    };
    let body = dereth_protocol::write_body(&ddd_data_body(contents, item.db)).ok()?;
    let len = u32::try_from(body.len()).ok()?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&body);
    Some(frame)
}

static SERVICE: OnceLock<Arc<Service>> = OnceLock::new();

/// Listen for streams on `addr` within `limits`, as the world's one listener.
///
/// # Errors
/// The address cannot be bound, the thread cannot start, or the world already listens.
pub fn start(addr: SocketAddr, limits: Limits) -> std::io::Result<SocketAddr> {
    let service = Service::listen(addr, limits)?;
    let bound = SocketAddr::new(addr.ip(), service.port());
    SERVICE.set(service).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "the patch stream already listens",
        )
    })?;
    Ok(bound)
}

/// The world's listener, once [`start`] listens.
#[must_use]
pub fn service() -> Option<&'static Arc<Service>> {
    SERVICE.get()
}

/// The world's stream port, once [`start`] listens.
#[must_use]
pub fn port() -> Option<u32> {
    service().map(|s| u32::from(s.port()))
}

/// [`Service::withdraw`] on the world's listener.
pub fn withdraw(token: &[u8; 16]) -> bool {
    service().is_some_and(|s| s.withdraw(token))
}

/// A session whose offer is past its deadline: unclaimed, its records go back to `0xF7E2`;
/// claimed, the stream carries them, and a decline brings back what it did not.
pub fn expire(w: &mut World, session: SessionId) {
    let now = w.now.utc;
    let Some(s) = w.sessions.get_mut(session) else {
        return;
    };
    let Some(p) = s.bulk.as_mut() else {
        return;
    };
    if p.claimed || now < p.deadline {
        return;
    }
    if withdraw(&p.token) {
        let items = std::mem::take(&mut p.items);
        s.bulk = None;
        let n = items.len();
        for (id, db) in items {
            s.add_to_ddd_queue(id, db);
        }
        log::info!("[DDD] a bulk patch stream was not collected: its {n} records go as 0xF7E2s");
    } else {
        p.claimed = true;
    }
}

/// The records of `pending` a client names in `0xF7EE`, by `(dat_file_type, dat_file_id, id)`,
/// or all of them when it names none, taken out of `pending` so that none goes twice.
pub fn take_declined(
    pending: &mut BulkPending,
    named: &[(u32, u32, u32)],
) -> Vec<(u32, DatDatabaseType)> {
    if named.is_empty() {
        return std::mem::take(&mut pending.items);
    }
    let named: HashSet<(DatDatabaseType, u32)> = named
        .iter()
        .filter_map(|&(t, f, id)| {
            let db = match (t, f) {
                (0, 1) => DatDatabaseType::Portal,
                (1, 2) => DatDatabaseType::Cell,
                (1, 3) => DatDatabaseType::Language,
                (t, 1) if t.cast_signed() == ddd_manager::HI_FI_STRING_AS_INT => {
                    DatDatabaseType::HighRes
                }
                _ => return None,
            };
            Some((db, id))
        })
        .collect();
    let (again, kept) = std::mem::take(&mut pending.items)
        .into_iter()
        .partition(|(id, db)| named.contains(&(*db, *id)));
    pending.items = kept;
    again
}
