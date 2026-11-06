//! The script library an admin's own retail client needs, served through the login's data download.
//!
//! The September 2013 client's admin commands go out encrypted, under a key only the original servers
//! could read. Its script library decides that, call by call, and the client takes a library record
//! from the data download and runs it. So this library differs from the install's in one operand of
//! each encrypted send: the same send in the clear. Nothing checks a library record's bytes.
//!
//! With it go two records no install holds, each an enumeration the client's master map names. One
//! holds the names of the `/ethereality` command's words: without it the client finds no value for
//! a word and never sends the command. Its values are this server's own
//! ([`crate::admin_calls::ethereal_value`]): what the original numbered them isn't held. The other
//! names the log type of an admin's answers: without it the client shows each answer under an
//! empty `[]`. Its one name is this server's choice, the channel the client's help calls `Admin`;
//! what else the original listed isn't held.
//!
//! It goes to an admin's account alone, once: the client lists the iterations its portal file has
//! had, and one holding the library's iteration is not sent it again. A client holding the iteration
//! before it holds the library already, and is sent only what that one lacked. It must run with its
//! data files writable (`-rodat off`, as the original launcher started it), and on a copy of the
//! install kept for this world: the library stays in the copy it was stored in.

use auberean_assets::strings::EnumMap;
use auberean_dat::Writer;
use auberean_protocol::login::{DataFileIterations, DownloadBegins, MissingIterations};
use auberean_protocol::types::IterationRun;
use auberean_protocol::ServerMessage;

use crate::admin_calls::{ethereal_value, LOG_TYPE};
use crate::download;

/// The library's record, and its kind.
pub const RECORD: u32 = 0x5600_0005;
pub const KIND: u32 = 0x4000_004F;

/// A call of a native: the machine's opcode, then which.
const CALL_NATIVE: u32 = 0x0C;
/// The natives that send an event encrypted and in the clear: their package, then which.
const SENDS: u32 = 0x7F;
const SEND_ENCRYPTED: u32 = SENDS << 16 | 0x37;
const SEND_PLAIN: u32 = SENDS << 16 | 0x36;

/// The `/ethereality` words' record, and its kind.
pub const ETHEREALITY_RECORD: u32 = 0x2300_00AE;
pub const ENUM_KIND: u32 = 0x4000_0028;

/// The log type names' record, of the same kind.
pub const LOG_TYPE_RECORD: u32 = 0x2300_0037;

/// The iteration it goes out as: above any the original data files had, and above the ones before
/// it, so a client that holds only one of those is sent what it lacks.
pub const ITERATION: u32 = 9_002;

/// The iteration before it, which carried all of it but the log type's names.
pub const PREVIOUS_ITERATION: u32 = 9_001;

/// How many sends the library this server serves changes, at the least: fewer means it is not the
/// library this was made for.
pub const LEAST_SENDS: usize = 170;

/// The library the client should have, packed.
#[derive(Debug, Clone, PartialEq)]
pub struct AdminLibrary {
    pub begins: ServerMessage,
    pub records: Vec<ServerMessage>,
    /// All a client holding [`PREVIOUS_ITERATION`] needs: the library itself takes minutes to send.
    pub newer_begins: ServerMessage,
    pub newer_records: Vec<ServerMessage>,
}

/// The record of the `/ethereality` command's words.
#[must_use]
pub fn ethereality_record() -> Vec<u8> {
    let mut w = Writer::new();
    EnumMap {
        id: ETHEREALITY_RECORD,
        base: 0,
        names: vec![
            (ethereal_value::NORMAL, "normal".to_owned()),
            (ethereal_value::ADMIN, "admin".to_owned()),
            (ethereal_value::SUPER, "super".to_owned()),
        ],
    }
    .write(&mut w);
    w.finish()
}

/// The record of the log type's names.
#[must_use]
pub fn log_type_record() -> Vec<u8> {
    let mut w = Writer::new();
    EnumMap {
        id: LOG_TYPE_RECORD,
        base: 0,
        names: vec![(LOG_TYPE, "Admin".to_owned())],
    }
    .write(&mut w);
    w.finish()
}

/// A library record with every encrypted send made a plain one: the new bytes and how many it
/// changed. Same size, same layout.
#[must_use]
pub fn plain(record: &[u8]) -> (Vec<u8>, usize) {
    let mut out = record.to_vec();
    let mut n = 0;
    let (from, to) = (SEND_ENCRYPTED.to_le_bytes(), SEND_PLAIN.to_le_bytes());
    let call = CALL_NATIVE.to_le_bytes();
    let mut i = 0;
    while i + 8 <= out.len() {
        if out[i..i + 4] == call && out[i + 4..i + 8] == from {
            out[i + 4..i + 8].copy_from_slice(&to);
            n += 1;
            i += 8;
        } else {
            i += 4;
        }
    }
    (out, n)
}

impl AdminLibrary {
    /// How many bytes a client that holds none of it is sent.
    #[must_use]
    pub fn bytes(&self) -> u32 {
        match &self.begins {
            ServerMessage::DownloadBegins(b) => b.bytes,
            _ => 0,
        }
    }

    /// The library made from the install's own record.
    ///
    /// # Errors
    /// The record is not one this was made for: too few sends to change.
    pub fn of(record: &[u8]) -> Result<Self, String> {
        let (changed, n) = plain(record);
        if n < LEAST_SENDS {
            return Err(format!(
                "the install's script library has {n} encrypted sends, fewer than the \
                 {LEAST_SENDS} of the library this server knows"
            ));
        }
        let packed = download::packed(download::PORTAL_FILE, KIND, RECORD, &changed);
        let enumeration =
            |id, record: &[u8]| download::packed(download::PORTAL_FILE, ENUM_KIND, id, record);
        let words = enumeration(ETHEREALITY_RECORD, &ethereality_record());
        let log_type = enumeration(LOG_TYPE_RECORD, &log_type_record());
        let begins = |records: &[&_], download: Vec<u32>| {
            let bytes: usize = records
                .iter()
                .map(|r: &&auberean_protocol::login::DataRecord| r.bytes.len())
                .sum();
            ServerMessage::DownloadBegins(DownloadBegins {
                bytes: u32::try_from(bytes).unwrap_or(u32::MAX),
                files: vec![MissingIterations {
                    dat_file: download::PORTAL_FILE,
                    iteration: ITERATION,
                    download,
                    purge: vec![],
                }],
            })
        };
        Ok(Self {
            begins: begins(
                &[&packed, &words, &log_type],
                vec![RECORD, ETHEREALITY_RECORD, LOG_TYPE_RECORD],
            ),
            newer_begins: begins(&[&log_type], vec![LOG_TYPE_RECORD]),
            newer_records: vec![ServerMessage::DataRecord(log_type.clone())],
            records: vec![
                ServerMessage::DataRecord(packed),
                ServerMessage::DataRecord(words),
                ServerMessage::DataRecord(log_type),
            ],
        })
    }

    /// What a client that told of its data is to be sent, in order: nothing when it holds the
    /// library, only what's newer when it holds the iteration before.
    #[must_use]
    pub fn to_send(
        &self,
        files: &[DataFileIterations],
    ) -> Option<(&ServerMessage, &[ServerMessage])> {
        if holds(files, ITERATION) {
            None
        } else if holds(files, PREVIOUS_ITERATION) {
            Some((&self.newer_begins, &self.newer_records))
        } else {
            Some((&self.begins, &self.records))
        }
    }
}

/// Has a client told of its data holding an iteration of the portal file?
fn holds(files: &[DataFileIterations], iteration: u32) -> bool {
    files
        .iter()
        .filter(|f| f.dat_file == download::PORTAL_FILE)
        .flat_map(|f| &f.iterations.runs)
        .any(|r| match *r {
            IterationRun::One(v) => v.cast_unsigned() == iteration,
            IterationRun::Run { count, first } => first <= iteration && iteration - first < count,
        })
}
