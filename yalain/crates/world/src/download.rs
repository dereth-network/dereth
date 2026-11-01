//! The records a client asks for that its own data files lack, served from the server's data
//! files, as the January 2017 servers served them.
//!
//! - A request names a record's kind and id. The kind says which file the record lives in: the
//!   cell file for the landscape's kinds, the language's file for the text and spoken kinds, the
//!   portal file for the rest (the client's own table of kinds, [`RecordType::info`]).
//! - A record the server's file holds is sent whole, as that file stores it, compressed when that
//!   makes it smaller; a landblock brings its landblock information right after it, unasked.
//! - A record the server does not hold is refused with the kind, the id and error 1.

use std::collections::BTreeMap;
use std::io::Write as _;

use auberean_dat::{DatFile, DatKind, RecordType};
use auberean_protocol::login::{DataRecord, DataRefused, DataRequest};
use auberean_protocol::{queue, ServerMessage};
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::Server;

/// The portal file, as a data answer names it.
pub const PORTAL_FILE: u64 = 0x1_0000_0000;
/// The first cell file.
pub const CELL_FILE: u64 = 0x2_0000_0001;
/// The English language's file.
pub const ENGLISH_FILE: u64 = 0x3_8000_0001;

/// The error a refusal carries.
pub const NOT_HELD: u32 = 1;

/// The data files the server serves records from.
pub trait DataFiles: std::fmt::Debug {
    /// The record `id` of the kind of file `file`, as that file stores it, if it holds one.
    fn read(&self, file: DatKind, id: u32) -> Option<Vec<u8>>;
}

/// Records held in memory, by file and id.
impl DataFiles for BTreeMap<(DatKind, u32), Vec<u8>> {
    fn read(&self, file: DatKind, id: u32) -> Option<Vec<u8>> {
        self.get(&(file, id)).cloned()
    }
}

/// An install's own data files: its portal file, its first cell file and its English file.
#[derive(Debug, Default, Clone)]
pub struct InstallFiles {
    pub portal: Option<DatFile>,
    pub cell: Option<DatFile>,
    pub english: Option<DatFile>,
}

impl DataFiles for InstallFiles {
    fn read(&self, file: DatKind, id: u32) -> Option<Vec<u8>> {
        let dat = match file {
            DatKind::Portal => self.portal.as_ref(),
            DatKind::Cell => self.cell.as_ref(),
            DatKind::Local => self.english.as_ref(),
            DatKind::None => None,
        }?;
        dat.read(id).ok()
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
pub fn answer<S: Store>(s: &Server<S>, net: &mut ServerNet, conn: ConnId, r: DataRequest) {
    for m in answers(s.data.as_deref(), r) {
        net.send(conn, queue::DATABASE, BlobKind::LOGIN, m.encode());
    }
}
