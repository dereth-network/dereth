//! The records a client asks for that its own data files lack, served from the server's data
//! files, as the January 2017 servers served them.
//!
//! - A request names a record's kind and id. The kind says which file the record lives in: the
//!   cell file for the landscape's kinds, the language's file for the text and spoken kinds, the
//!   portal file for the rest (the client's own table of kinds, [`RecordType::info`]).
//! - A record the server's file holds is sent whole, as that file stores it, compressed when that
//!   makes it smaller; a landblock brings its landblock information right after it, unasked.
//! - A record the server does not hold is refused with the kind, the id and error 1.
//!
//! Answering never holds up the world. A very long record is compressed as the server starts
//! ([`prepare`]) and its answer costs little after that. What else answers compress, and read, is
//! kept to a budget, each connection's and the world's: a request past it is refused, as for a
//! record not held. And a connection has only so much waiting to be sent: a request past that is
//! let go. Nothing the 2005 to 2017 recordings hold comes near any of these.

use std::collections::{BTreeMap, HashMap};
use std::io::Write as _;

use auberean_dat::{DatFile, DatKind, RecordType};
use auberean_protocol::login::{DataRecord, DataRefused, DataRequest};
use auberean_protocol::{queue, ServerMessage};
use dereth_primitives::DataId;
use yalain_net::{BlobKind, ConnId, LocalTime, ServerNet};
use yalain_store::Store;

use crate::{Clock, Server};

/// The portal file, as a data answer names it.
pub const PORTAL_FILE: u64 = 0x1_0000_0000;
/// The first cell file.
pub const CELL_FILE: u64 = 0x2_0000_0001;
/// The English language's file.
pub const ENGLISH_FILE: u64 = 0x3_8000_0001;

/// The error a refusal carries.
pub const NOT_HELD: u32 = 1;

/// A record this long or longer is compressed as the server starts.
pub const PREPARED_FROM: usize = 4 * 1024 * 1024;
/// The bytes of records a connection's answers may take at once, and each second after that.
pub const CONNECTION_AT_ONCE: f64 = 8.0 * 1024.0 * 1024.0;
pub const CONNECTION_PER_SECOND: f64 = 4.0 * 1024.0 * 1024.0;
/// The same for the whole world's answers.
pub const WORLD_AT_ONCE: f64 = 16.0 * 1024.0 * 1024.0;
pub const WORLD_PER_SECOND: f64 = 16.0 * 1024.0 * 1024.0;
/// What an answer counts for at the least, whatever its length.
pub const LEAST_COST: f64 = 1024.0;
/// The fragments a connection may have waiting to be sent before its requests are let go.
pub const MOST_WAITING: usize = 65_000;

/// A budget that fills with time. It may go below nothing: what is begun is finished, and nothing
/// more is begun until it has filled again.
#[derive(Debug, Clone, Copy)]
struct Budget {
    left: f64,
    at: LocalTime,
}

impl Budget {
    const fn new(left: f64, at: LocalTime) -> Self {
        Self { left, at }
    }

    /// Is there any left now, for one that holds `most` and fills by `per_second`?
    fn has_any(&mut self, most: f64, per_second: f64, now: LocalTime) -> bool {
        self.left = (self.left + (now.0 - self.at.0).max(0.0) * per_second).min(most);
        self.at = now;
        self.left > 0.0
    }
}

/// What answering has cost, and the answers made ahead of time.
#[derive(Debug, Default)]
pub struct Downloads {
    /// By file and record: its length once inflated (0 when it is sent as it is), and its bytes.
    prepared: BTreeMap<(DatKind, u32), (u32, Vec<u8>)>,
    world: Option<Budget>,
    connections: HashMap<ConnId, Budget>,
    /// Bytes of records compressed for answers so far, as the server runs.
    pub compressed: u64,
}

impl Downloads {
    /// A connection is gone.
    pub fn closed(&mut self, conn: ConnId) {
        self.connections.remove(&conn);
    }

    /// May a connection's request be answered now?
    fn affordable(&mut self, conn: ConnId, now: LocalTime) -> bool {
        let world = self
            .world
            .get_or_insert_with(|| Budget::new(WORLD_AT_ONCE, now));
        let connection = self
            .connections
            .entry(conn)
            .or_insert_with(|| Budget::new(CONNECTION_AT_ONCE, now));
        // Both are filled to now, whichever is empty.
        let world_ok = world.has_any(WORLD_AT_ONCE, WORLD_PER_SECOND, now);
        let connection_ok = connection.has_any(CONNECTION_AT_ONCE, CONNECTION_PER_SECOND, now);
        world_ok && connection_ok
    }

    /// What an answer has cost.
    fn spend(&mut self, conn: ConnId, bytes: usize) {
        #[allow(clippy::cast_precision_loss)]
        let cost = (bytes as f64).max(LEAST_COST);
        if let Some(b) = &mut self.world {
            b.left -= cost;
        }
        if let Some(b) = self.connections.get_mut(&conn) {
            b.left -= cost;
        }
    }
}

/// The data files the server serves records from.
pub trait DataFiles: std::fmt::Debug {
    /// The record `id` of the kind of file `file`, as that file stores it, if it holds one.
    fn read(&self, file: DatKind, id: u32) -> Option<Vec<u8>>;

    /// The records of [`PREPARED_FROM`] or longer.
    fn very_long(&self) -> Vec<(DatKind, u32)>;

    /// Whether a world's overlay holds a word on this record, which a reload may change.
    fn overlaid(&self, _file: DatKind, _id: u32) -> bool {
        false
    }
}

/// Records held in memory, by file and id.
impl DataFiles for BTreeMap<(DatKind, u32), Vec<u8>> {
    fn read(&self, file: DatKind, id: u32) -> Option<Vec<u8>> {
        self.get(&(file, id)).cloned()
    }

    fn very_long(&self) -> Vec<(DatKind, u32)> {
        self.iter()
            .filter(|(_, record)| record.len() >= PREPARED_FROM)
            .map(|(key, _)| *key)
            .collect()
    }
}

/// An install's own data files: its portal file, its first cell file and its English file.
#[derive(Debug, Default, Clone)]
pub struct InstallFiles {
    pub portal: Option<DatFile>,
    pub cell: Option<DatFile>,
    pub english: Option<DatFile>,
}

impl InstallFiles {
    const fn of(&self, file: DatKind) -> Option<&DatFile> {
        match file {
            DatKind::Portal => self.portal.as_ref(),
            DatKind::Cell => self.cell.as_ref(),
            DatKind::Local => self.english.as_ref(),
            DatKind::None => None,
        }
    }
}

impl DataFiles for InstallFiles {
    fn read(&self, file: DatKind, id: u32) -> Option<Vec<u8>> {
        self.of(file)?.read(id).ok()
    }

    fn very_long(&self) -> Vec<(DatKind, u32)> {
        [
            (DatKind::Portal, &self.portal),
            (DatKind::Cell, &self.cell),
            (DatKind::Local, &self.english),
        ]
        .into_iter()
        .filter_map(|(kind, dat)| dat.as_ref().map(|d| (kind, d)))
        .flat_map(|(kind, d)| {
            d.entries()
                .filter(|e| e.size as usize >= PREPARED_FROM)
                .map(move |e| (kind, e.id))
        })
        .collect()
    }

    fn overlaid(&self, file: DatKind, id: u32) -> bool {
        self.of(file)
            .and_then(|dat| dat.shared().layer())
            .is_some_and(|layer| layer.touches(DataId(id)))
    }
}

/// The file a kind of record lives in, and the name a data answer gives that file; `None` for a
/// kind no client file keeps.
#[must_use]
pub fn file_of(kind: u32) -> Option<(DatKind, u64)> {
    match RecordType(kind).info()?.dat {
        DatKind::Portal => Some((DatKind::Portal, PORTAL_FILE)),
        DatKind::Cell => Some((DatKind::Cell, CELL_FILE)),
        DatKind::Local => Some((DatKind::Local, ENGLISH_FILE)),
        DatKind::None => None,
    }
}

/// A record as a data answer carries it: compressed when that is smaller than the record, else
/// as it is.
#[must_use]
pub fn packed(dat_file: u64, kind: u32, id: u32, record: &[u8]) -> DataRecord {
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let small = z
        .write_all(record)
        .and_then(|()| z.finish())
        .ok()
        .filter(|c| c.len() < record.len());
    match (small, u32::try_from(record.len())) {
        (Some(bytes), Ok(size)) => DataRecord {
            dat_file,
            kind,
            id,
            size,
            bytes,
        },
        _ => DataRecord {
            dat_file,
            kind,
            id,
            size: 0,
            bytes: record.to_vec(),
        },
    }
}

/// The answers to one request: the record and what comes with it, or the refusal.
#[must_use]
pub fn answers(files: Option<&dyn DataFiles>, r: DataRequest) -> Vec<ServerMessage> {
    let refused = || {
        vec![ServerMessage::DataRefused(DataRefused {
            kind: r.kind,
            id: r.id,
            error: NOT_HELD,
        })]
    };
    let (Some(files), Some((file, name))) = (files, file_of(r.kind)) else {
        return refused();
    };
    let Some(record) = files.read(file, r.id) else {
        return refused();
    };
    let mut out = vec![ServerMessage::DataRecord(packed(
        name, r.kind, r.id, &record,
    ))];
    if RecordType(r.kind) == RecordType::LANDBLOCK {
        let info = (r.id & 0xFFFF_0000) | 0xFFFE;
        if let Some(rec) = files.read(file, info) {
            out.push(ServerMessage::DataRecord(packed(
                name,
                RecordType::LANDBLOCK_INFO.0,
                info,
                &rec,
            )));
        }
    }
    out
}

/// Answer a client's request for a record on its login connection.
pub fn answer<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    r: DataRequest,
    clock: Clock,
) {
    // A refusal would only wait behind what is waiting.
    if net.large_waiting(conn) >= MOST_WAITING {
        return;
    }
    let answers = if s.downloads.affordable(conn, clock.local) {
        answers_counted(s, conn, r)
    } else {
        vec![refusal(r)]
    };
    for m in answers {
        net.send(conn, queue::DATABASE, BlobKind::LOGIN, m.encode());
    }
}

/// Compress the [`PREPARED_FROM`] records now, before the world runs. What a world's overlay holds
/// isn't kept: a reload may change it.
pub fn prepare<S: Store>(s: &mut Server<S>) {
    let Some(files) = s.data.as_deref() else {
        return;
    };
    for (file, id) in files.very_long() {
        if files.overlaid(file, id) {
            continue;
        }
        if let Some(record) = files.read(file, id) {
            let p = packed(0, 0, id, &record);
            s.downloads.prepared.insert((file, id), (p.size, p.bytes));
        }
    }
    tracing::info!(
        "{} long data records compressed",
        s.downloads.prepared.len()
    );
}

/// The answers to one request, what they cost counted.
fn answers_counted<S: Store>(
    s: &mut Server<S>,
    conn: ConnId,
    r: DataRequest,
) -> Vec<ServerMessage> {
    let Some((file, name)) = file_of(r.kind) else {
        s.downloads.spend(conn, 0);
        return vec![refusal(r)];
    };
    let Some(record) = record_answer(s, conn, (file, name), r.kind, r.id) else {
        return vec![refusal(r)];
    };
    let mut out = vec![ServerMessage::DataRecord(record)];
    if RecordType(r.kind) == RecordType::LANDBLOCK {
        let info = (r.id & 0xFFFF_0000) | 0xFFFE;
        let with = RecordType::LANDBLOCK_INFO.0;
        if let Some(record) = record_answer(s, conn, (file, name), with, info) {
            out.push(ServerMessage::DataRecord(record));
        }
    }
    out
}

/// A record's answer, prepared or compressed now, what it cost counted.
fn record_answer<S: Store>(
    s: &mut Server<S>,
    conn: ConnId,
    (file, name): (DatKind, u64),
    kind: u32,
    id: u32,
) -> Option<DataRecord> {
    let overlaid = s.data.as_deref().is_some_and(|f| f.overlaid(file, id));
    if let Some((size, bytes)) = s.downloads.prepared.get(&(file, id)).filter(|_| !overlaid) {
        let (size, bytes) = (*size, bytes.clone());
        s.downloads.spend(conn, 0);
        return Some(DataRecord {
            dat_file: name,
            kind,
            id,
            size,
            bytes,
        });
    }
    let record = s.data.as_deref().and_then(|f| f.read(file, id));
    s.downloads.spend(conn, record.as_ref().map_or(0, Vec::len));
    let record = record?;
    s.downloads.compressed += record.len() as u64;
    Some(packed(name, kind, id, &record))
}

/// A record refused, as one not held.
const fn refusal(r: DataRequest) -> ServerMessage {
    ServerMessage::DataRefused(DataRefused {
        kind: r.kind,
        id: r.id,
        error: NOT_HELD,
    })
}
