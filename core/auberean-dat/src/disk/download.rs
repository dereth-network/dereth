//! The data download: the server's questions about which data versions the client holds, and
//! the records it sends to bring them up to date, stored as they come.
//!
//! The flow, by the server's messages:
//! 1. **Asked which data it holds**: the client answers with each open file's data versions
//!    ([`Download::on_server_interrogation`]). Records that arrive from now on are stored at once
//!    and remembered as early.
//! 2. **A download begins**, listing per data file the version to reach, the records to fetch and
//!    the records to drop ([`Download::on_begin_ddd`]): the drops are made (a landblock with
//!    every record of its block), a file with nothing to fetch takes its new version at once, and
//!    every record to fetch is waited for; those that came early are checked off. With nothing to
//!    wait for, the client says it is done.
//! 3. **Each record** ([`Download::on_data`]) is stored in its file; the last one a file waited
//!    for gives that file its new version, and the last of all ends the download from the
//!    client's side. A record that does not unpack is asked for again; one that does not store
//!    stops the client.
//! 4. **The server ends it** ([`Download::on_end_ddd`]): the lists are let go and the game runs.
//!
//! No I/O and no threads: the files are the [`StoreSet`] given to each call, and what the client
//! must do in turn (tell the connecting screen, send a message, ask for a record, stop) is queued
//! in [`Download::out`] for the host to act on, in order.
//!
//! **What the client's runtime calls**, message by message (the login connection's data queue):
//! - `DatInterrogation` (`0x10037`): [`Download::on_server_interrogation`]; its lists, with the
//!   language, are the `DatInterrogationResponse` (`0x10038`) to send; none sends nothing.
//! - `BeginDDD` (`0x1003A`): [`Download::on_begin_ddd`] with the byte count and each file's part
//!   made by [`MissingIteration::from_wire`].
//! - `DatData` (`0x10035`): [`Download::on_data`] with the record inflated (none when it does not
//!   inflate) and its length word.
//! - `DatRequestFailed` (`0x10036`): [`Download::on_refused`]; the patch-time notice
//!   (`0x10039`): [`Download::on_waiting`]; `EndDDD` (`0x1003B`): [`Download::on_end_ddd`].
//!
//! Then each [`Output`], in order: a notice to the connecting screen's data events; `SendEnd` as
//! `EndDDD` (`0x1003B`) to the server; `Request` as `DatRequest` (`0x10034`); `Refused` to the
//! request waiting for that record; `Fatal` as the notice, then the client stops.

use std::collections::BTreeMap;

use super::intset::VersionSet;
use std::sync::Arc;

use super::shared::{SharedStore, StoreSet};
use crate::qualified::{divine_cell_type, divine_type, QualifiedId};
use crate::types::RecordType;

/// Where the download stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DddState {
    #[default]
    Idle = 0,
    /// Asked which data the client holds; no download begun.
    InterrogationReceived = 1,
    /// A download is under way.
    Patching = 2,
    /// The download is over.
    RunTime = 3,
}

/// What the connecting screen is told, by number, with a value.
pub mod event {
    /// Asked which data the client holds.
    pub const INTERROGATION: u32 = 0;
    /// Waiting in line (the value: the place).
    pub const WAITING: u32 = 1;
    /// Downloading (the value: the bytes still to come).
    pub const PATCHING: u32 = 2;
    /// Done.
    pub const DONE: u32 = 3;
    /// A record came (the value: its length).
    pub const PROGRESS: u32 = 4;
}

/// The notice shown, before the client stops, when a downloaded record does not store.
pub const SAVE_FAILED_NOTICE: u32 = 0xCF;

/// The data sets, as a data file's id holds them in its high word.
pub const PORTAL_SET: u32 = 1;
pub const CELL_SET: u32 = 2;
pub const LOCAL_SET: u32 = 3;

/// What the host must do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Tell the connecting screen ([`event`]).
    Notice { event: u32, value: u32 },
    /// Send the end of the download.
    SendEnd,
    /// Ask the server for this record again.
    Request(QualifiedId),
    /// The server could not send this record: the request waiting for it fails.
    Refused(QualifiedId),
    /// Show this notice and stop.
    Fatal(u32),
}

/// One data file's part of a download: the version it reaches, the records to fetch and those to
/// drop.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissingIteration {
    pub dat_file: u64,
    pub iteration: u32,
    pub download: Vec<QualifiedId>,
    pub purge: Vec<QualifiedId>,
}

/// Each id with the type of record it is, as a download's lists carry ids only: in a cell file by
/// its low word, elsewhere by its range; an id no type claims is the set's own default (a
/// script library in the portal, a landblock in a cell file, a string table in a local file, else
/// none).
// D2-12848
#[must_use]
pub fn stream_ids(ids: &[u32], set: u32) -> Vec<QualifiedId> {
    ids.iter()
        .map(|&id| {
            let found = if set == CELL_SET {
                divine_cell_type((id & 0xFFFF) as u16)
            } else {
                divine_type(id)
            };
            let kind = if found != 0 {
                RecordType(found)
            } else {
                match set {
                    PORTAL_SET => RecordType::SCRIPT_LIBRARY,
                    CELL_SET => RecordType::LANDBLOCK,
                    LOCAL_SET => RecordType::STRING_TABLE,
                    _ => RecordType(0),
                }
            };
            QualifiedId { id, kind: kind.0 }
        })
        .collect()
}

/// A downloaded record's bytes as the client stores them: as sent when `size` is 0, else
/// inflated (a zlib stream) to `size` bytes; none when it does not inflate. (One that inflates
/// short is refused here; the client keeps it, the rest of its size unwritten.)
// D2-12841
#[must_use]
pub fn cache_pack(size: u32, bytes: &[u8]) -> Option<Vec<u8>> {
    if size == 0 {
        return Some(bytes.to_vec());
    }
    dereth_dat::inflate_zlib(bytes, size as usize).ok()
}

impl MissingIteration {
    /// A file's part as the server sends it: its id, the version, and the ids to fetch and to
    /// drop, each typed by [`stream_ids`] in the file's data set.
    // D2-05169
    #[must_use]
    pub fn from_wire(dat_file: u64, iteration: u32, download: &[u32], purge: &[u32]) -> Self {
        let set = (dat_file >> 32) as u32;
        Self {
            dat_file,
            iteration,
            download: stream_ids(download, set),
            purge: stream_ids(purge, set),
        }
    }
}

/// The client's side of the data download.
#[derive(Debug, Default)]
pub struct Download {
    pub state: DddState,
    pub missing: Vec<MissingIteration>,
    /// Each record waited for, and the file's part it belongs to (an index into `missing`).
    pub pending: BTreeMap<QualifiedId, usize>,
    /// The records stored before the download began, and their lengths' sum.
    pub early_saves: Vec<QualifiedId>,
    pub early_bytes: u32,
    pub out: Vec<Output>,
}

/// The slot of the file whose id is `dat_file`, 4 when none of the four is (the first slot
/// when no file is open).
fn file_index(set: &StoreSet, dat_file: u64) -> usize {
    if set.by_slot.iter().all(Option::is_none) {
        return 0;
    }
    set.by_slot
        .iter()
        .position(|s| s.as_ref().is_some_and(|s| s.dat_file_id() == dat_file))
        .unwrap_or(4)
}

/// Add `iteration` to a file's held versions and store them.
fn take_version(store: Option<Arc<SharedStore>>, iteration: u32) {
    if let Some(s) = store {
        let mut file = s.lock();
        let mut held = s
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        held.add(iteration.cast_signed());
        held.sort();
        file.save_iteration_list(&mut held);
    }
}

impl Download {
    /// A download not begun.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Back to idle (the cache reset's part here; its requests are the runtime's to let go).
    // D2-05083
    pub fn reset_cache(&mut self) {
        self.state = DddState::Idle;
    }

    /// Tell the connecting screen.
    // D2-05130
    pub fn notify_ddd_event(&mut self, event: u32, value: u32) {
        self.out.push(Output::Notice { event, value });
    }

    /// Asked which data the client holds: records from now on are early. The answer: each open
    /// file's id and its data versions, read from the file, in the files' order; none (nothing is
    /// sent) when any open file's list does not read.
    // D2-12851
    pub fn on_server_interrogation(&mut self, set: &StoreSet) -> Option<Vec<(u64, VersionSet)>> {
        self.state = DddState::InterrogationReceived;
        self.early_saves.clear();
        self.early_bytes = 0;
        let mut lists = Vec::new();
        let mut ok = true;
        for s in set.by_slot.iter().flatten() {
            if !s.is_initialized() {
                continue;
            }
            let id = s.dat_file_id();
            match s.lock().load_iteration_list() {
                Some(v) => lists.push((id, v)),
                None => {
                    lists.push((id, VersionSet::new()));
                    ok = false;
                }
            }
        }
        ok.then_some(lists)
    }

    /// A download begins: `bytes` to come in all, and each file's part. The connecting screen is
    /// told the bytes still to come (less those that came early), the drops are made, and the
    /// waiting begins.
    // D2-05139, D2-05084, D2-05085
    pub fn on_begin_ddd(&mut self, set: &StoreSet, bytes: u32, missing: Vec<MissingIteration>) {
        self.state = DddState::Patching;
        self.missing = missing;
        let left = bytes.wrapping_sub(self.early_bytes);
        if left != 0 {
            self.notify_ddd_event(event::PATCHING, left);
        }
        self.worker_execute_begin_ddd_request(set);
        self.on_begin_ddd_request_finished(set);
    }

    /// The drops of every file's part: a landblock goes with every record of its block (by
    /// mask), any other record alone; a part for a file not open drops nothing.
    // D2-05112
    pub fn worker_execute_begin_ddd_request(&mut self, set: &StoreSet) {
        for mi in &self.missing {
            if file_index(set, mi.dat_file) >= 4 {
                continue;
            }
            for q in &mi.purge {
                if q.kind == RecordType::LANDBLOCK.0 {
                    if let Some(s) = set.get_disk_controller(*q, mi.dat_file) {
                        s.delete_data_by_mask(q.id, 0xFFFF_0000);
                    }
                } else {
                    let _ = set.purge_data(*q);
                }
            }
        }
    }

    /// After the drops: a file's part with nothing to fetch gives the file its version now;
    /// every record to fetch is waited for. With nothing to wait for the client says it is done;
    /// else the records that came early are checked off.
    // D2-05183
    pub fn on_begin_ddd_request_finished(&mut self, set: &StoreSet) {
        for (n, mi) in self.missing.iter().enumerate() {
            let i = file_index(set, mi.dat_file);
            if i >= 4 {
                continue;
            }
            if mi.download.is_empty() {
                take_version(set.by_slot[i].clone(), mi.iteration);
            } else {
                for q in &mi.download {
                    self.pending.entry(*q).or_insert(n);
                }
            }
        }
        if self.pending.is_empty() {
            self.out.push(Output::SendEnd);
        } else {
            for q in self.early_saves.clone() {
                self.remove_pending_download(set, q);
            }
        }
    }

    /// A record waited for has come: off its part's list (matched by id) and out of the waiting;
    /// the part's last record gives its file the new version, and the last record of all ends the
    /// download from the client's side. Whether it was waited for.
    // D2-05157
    pub fn remove_pending_download(&mut self, set: &StoreSet, q: QualifiedId) -> bool {
        let Some(&n) = self.pending.get(&q) else {
            return false;
        };
        let mi = &mut self.missing[n];
        if let Some(k) = mi.download.iter().position(|d| d.id == q.id) {
            mi.download.swap_remove(k);
            if mi.download.is_empty() {
                let (dat_file, iteration) = (mi.dat_file, mi.iteration);
                take_version(set.get_disk_controller(q, dat_file), iteration);
            }
        }
        self.pending.remove(&q);
        if self.pending.is_empty() {
            self.out.push(Output::SendEnd);
        }
        true
    }

    /// A record from the server (`data` none when it did not unpack): stored in its file
    /// (`dat_file`, or the one its type belongs to), counted early before a download begins, and
    /// the connecting screen told its `length`. One that did not unpack is asked for again.
    /// Whether it was stored, when a store was tried (the data cache's requests take a record
    /// stored).
    // D2-05156
    pub fn on_data(
        &mut self,
        set: &StoreSet,
        dat_file: u64,
        q: QualifiedId,
        length: u32,
        data: Option<&[u8]>,
    ) -> Option<bool> {
        let Some(data) = data else {
            self.out.push(Output::Request(q));
            return None;
        };
        if data.is_empty() {
            return None;
        }
        if self.state == DddState::InterrogationReceived {
            self.early_saves.push(q);
            self.early_bytes = self.early_bytes.wrapping_add(length);
        }
        self.notify_ddd_event(event::PROGRESS, length);
        let stored = set.worker_execute_save_request(q, dat_file, data);
        self.on_save_request_finished(set, q, stored);
        Some(stored)
    }

    /// The server could not send a record: the request for it fails.
    // D2-12805
    pub fn on_refused(&mut self, q: QualifiedId) {
        self.out.push(Output::Refused(q));
    }

    /// Waiting in line at `place`.
    pub fn on_waiting(&mut self, place: u32) {
        self.notify_ddd_event(event::WAITING, place);
    }

    /// A record's store finished: one that failed shows its notice and stops the client; during a
    /// download, it is checked off.
    // D2-05173
    pub fn on_save_request_finished(&mut self, set: &StoreSet, q: QualifiedId, stored: bool) {
        if !stored {
            self.out.push(Output::Fatal(SAVE_FAILED_NOTICE));
            return;
        }
        if self.state == DddState::Patching {
            self.remove_pending_download(set, q);
        }
    }

    /// The server ends the download: the parts, the waiting and the early records let go, and
    /// the game runs.
    // D2-05142
    pub fn on_end_ddd(&mut self) {
        self.missing.clear();
        self.pending.clear();
        self.early_saves.clear();
        self.state = DddState::RunTime;
        self.notify_ddd_event(event::DONE, 0);
    }
}
