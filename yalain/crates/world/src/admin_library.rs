//! The script library an admin's own retail client needs, served through the login's data download.
//!
//! The September 2013 client's admin commands go out encrypted, under a key only the original servers
//! could read. Its script library decides that, call by call, and the client takes a library record
//! from the data download and runs it. So this library differs from the install's in one operand of
//! each encrypted send: the same send in the clear. Nothing checks a library record's bytes.
//!
//! It goes to an admin's account alone, once: the client lists the iterations its portal file has
//! had, and one holding the library's iteration is not sent it again. It must run with its data
//! files writable (`-rodat off`, as the original launcher started it), and on a copy of the install
//! kept for this world: the library stays in the copy it was stored in.

use auberean_protocol::login::{DataFileIterations, DownloadBegins, MissingIterations};
use auberean_protocol::types::IterationRun;
use auberean_protocol::ServerMessage;

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

/// The iteration it goes out as: above any the original data files had.
pub const ITERATION: u32 = 9_000;

/// How many sends the library this server serves changes, at the least: fewer means it is not the
/// library this was made for.
pub const LEAST_SENDS: usize = 170;

/// The library the client should have, packed.
#[derive(Debug, Clone, PartialEq)]
pub struct AdminLibrary {
    pub begins: ServerMessage,
    pub record: ServerMessage,
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
        Ok(Self {
            begins: ServerMessage::DownloadBegins(DownloadBegins {
                bytes: u32::try_from(packed.bytes.len()).unwrap_or(u32::MAX),
                files: vec![MissingIterations {
                    dat_file: download::PORTAL_FILE,
                    iteration: ITERATION,
                    download: vec![RECORD],
                    purge: vec![],
                }],
            }),
            record: ServerMessage::DataRecord(packed),
        })
    }

    /// Has a client told of its data holding this library already?
    #[must_use]
    pub fn held_by(&self, files: &[DataFileIterations]) -> bool {
        files
            .iter()
            .filter(|f| f.dat_file == download::PORTAL_FILE)
            .flat_map(|f| &f.iterations.runs)
            .any(|r| match *r {
                IterationRun::One(v) => v.cast_unsigned() == ITERATION,
                IterationRun::Run { count, first } => {
                    first <= ITERATION && ITERATION - first < count
                }
            })
    }
}
