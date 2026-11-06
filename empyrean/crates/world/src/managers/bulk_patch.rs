//! Not ACE (V509): the patch as one TCP stream, for a client that asks for it.
//!
//! A client patched to a world with a data overlay that sets `FLAG_BULK` in its interrogation
//! response is sent `0xF7ED` after `0xF7E7`, in place of the patch's `0xF7E2`s: a port and a
//! single-use token for a ticket that holds the records it lacks. It connects over TCP, sends the
//! magic, the version and the token, and reads every record as a `DddData` body behind its length
//! as a `u32`, until a length of 0. TCP's own flow control is the only limit on the rate; the
//! records go in whatever order the workers that build them finish. A ticket no client claims in
//! [`TICKET_SECONDS`], and whatever a client that could not finish names in `0xF7EE`, goes back
//! to the rate-limited `0xF7E2` queue, so a client that cannot reach the port is still patched.

use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use dereth_protocol::admin::{BULK_MAGIC, BULK_VERSION};
use empyrean_dat::{DatDatabaseType, DatManager};

use crate::managers::ddd_manager;
use crate::network::game_messages::messages::game_message_ddd_data_message::ddd_data_body;
use crate::World;
use empyrean_net::SessionId;

/// How long a ticket waits for its client to connect before its records go back to `0xF7E2`.
pub const TICKET_SECONDS: f64 = 30.0;
/// A ticket still unclaimed this long after its session gave it up is dropped.
const STALE: Duration = Duration::from_secs(600);
/// The zlib level the stream deflates a compressed record at: fast, as the stream is.
const LEVEL: i32 = 6;
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// One record of a ticket: the file, its dat, and whether it goes compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BulkItem {
    pub id: u32,
    pub db: DatDatabaseType,
    pub compress: bool,
}

/// A session's offered stream: its ticket's token, when it goes back to `0xF7E2` unless claimed,
/// whether a client claimed it, and its records for that fallback.
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
    made: Instant,
}

static PORT: AtomicU32 = AtomicU32::new(0);
static TICKETS: LazyLock<Mutex<HashMap<[u8; 16], Ticket>>> = LazyLock::new(Default::default);

/// The stream's port, once [`start`] listens.
#[must_use]
pub fn port() -> Option<u32> {
    match PORT.load(Ordering::Relaxed) {
        0 => None,
        p => Some(p),
    }
}

/// Listen for streams on `addr`.
///
/// # Errors
/// The address cannot be bound, or the thread cannot start.
pub fn start(addr: SocketAddr) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let bound = listener.local_addr()?;
    std::thread::Builder::new()
        .name("Bulk Patch".to_owned())
        .spawn(move || {
            for conn in listener.incoming() {
                let Ok(stream) = conn else { continue };
                let _ = std::thread::Builder::new()
                    .name("Bulk Patch Stream".to_owned())
                    .spawn(move || {
                        let peer = stream.peer_addr().ok();
                        if let Err(e) = serve(stream) {
                            log::info!("[DDD] bulk patch stream to {peer:?} ended: {e}");
                        }
                    });
            }
        })?;
    PORT.store(u32::from(bound.port()), Ordering::Relaxed);
    Ok(bound)
}

/// A ticket for `items` from `dats`, for `account`: its token.
#[must_use]
pub fn offer(items: Vec<BulkItem>, dats: Arc<DatManager>, account: &str) -> Option<[u8; 16]> {
    let mut token = [0u8; 16];
    getrandom::fill(&mut token).ok()?;
    let mut tickets = TICKETS.lock().ok()?;
    tickets.retain(|_, t| t.made.elapsed() < STALE);
    tickets.insert(
        token,
        Ticket {
            items,
            dats,
            account: account.to_owned(),
            made: Instant::now(),
        },
    );
    Some(token)
}

/// Take back a ticket no client has claimed; false when one has, or it is gone.
pub fn withdraw(token: &[u8; 16]) -> bool {
    TICKETS.lock().is_ok_and(|mut t| t.remove(token).is_some())
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

/// The records of `pending` a client names in `0xF7EE`, by `(dat_file_type, dat_file_id, id)`.
#[must_use]
pub fn declined(pending: &BulkPending, named: &[(u32, u32, u32)]) -> Vec<(u32, DatDatabaseType)> {
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
    pending
        .items
        .iter()
        .copied()
        .filter(|(id, db)| named.contains(&(*db, *id)))
        .collect()
}

fn serve(mut stream: TcpStream) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    stream.set_read_timeout(Some(HELLO_TIMEOUT))?;
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
    let ticket = TICKETS
        .lock()
        .ok()
        .and_then(|mut t| t.remove(&token))
        .ok_or_else(|| Error::new(ErrorKind::PermissionDenied, "no such ticket"))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let started = Instant::now();
    let (records, bytes) = send(stream, &ticket)?;
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

/// Every record of `ticket` down `stream`, built on several threads, then a length of 0.
fn send(stream: TcpStream, ticket: &Ticket) -> std::io::Result<(usize, u64)> {
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism()
        .map_or(4, std::num::NonZero::get)
        .min(8);
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(64);
    std::thread::scope(|s| {
        for _ in 0..workers {
            let tx = tx.clone();
            let next = &next;
            s.spawn(move || {
                while let Some(item) = ticket.items.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let level = item.compress.then_some(LEVEL);
                    let Some(contents) =
                        ddd_manager::dat_file_contents(&ticket.dats, item.id, item.db, level)
                    else {
                        log::warn!("[DDD] bulk patch: 0x{:08X} could not be read", item.id);
                        continue;
                    };
                    let Ok(body) = dereth_protocol::write_body(&ddd_data_body(contents, item.db))
                    else {
                        continue;
                    };
                    let mut frame = Vec::with_capacity(4 + body.len());
                    frame.extend_from_slice(&u32::try_from(body.len()).unwrap_or(0).to_le_bytes());
                    frame.extend_from_slice(&body);
                    if tx.send(frame).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut out = BufWriter::with_capacity(1 << 20, stream);
        let (mut records, mut bytes) = (0usize, 0u64);
        for frame in rx {
            out.write_all(&frame)?;
            records += 1;
            bytes += frame.len() as u64;
        }
        out.write_all(&0u32.to_le_bytes())?;
        out.flush()?;
        Ok((records, bytes))
    })
}
