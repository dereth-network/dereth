//! The data download, as the runtime runs it: the server's messages on the data queue handed to
//! the client's own download (`auberean_dat::disk::download`), and what it asks done in turn sent
//! to the server or kept for the connecting screen.
//!
//! The files the download reads and stores into are the client's data files as its one data
//! cache holds them ([`crate::cache::DataCache`]): opened over the files as they are and written
//! in memory for the session. A record the server sends is stored there, the files on disk are
//! never changed, and every part of the client reads the stored record from then on.
//!
//! The data cache's requests ([`auberean_assets::requests`]) run over the same files and the
//! same records' caches: a record asked for that is in neither memory nor the files is asked of
//! the server (`DatRequest`), and the record it sends, once stored, or its refusal
//! (`DatRequestFailed`), finishes the request. The cache is given time once a frame
//! ([`DataDownload::use_time`]); what its askers are told is kept in [`DataDownload::told`].
//! A saved script state the cache loads names its records through the client's own scripts, which
//! the client lends the calls that can finish one ([`StateRecords`]).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use auberean_assets::cache::AnyObj;
use auberean_assets::requests::{Batch, Event, Listener, Outcome, Requests, Sources, StateRecords};
use auberean_client_model::Effect;
use auberean_dat::disk::download::{cache_pack, Download, MissingIteration, Output, PORTAL_SET};
use auberean_dat::disk::{StoreSet, VersionSet};
use auberean_dat::{QualifiedId, Writer};
use auberean_protocol::login::{DataFileIterations, DataHeld, DataRequest, Empty};
use auberean_protocol::types::IterationSet;
use auberean_protocol::wire::{Field, Reader};
use auberean_protocol::ClientMessage;

use crate::cache::DataCache;
use crate::data::DataFiles;
use crate::strings::{self, StringDownloads};

/// The portal file, as the server's messages name it.
const PORTAL_FILE: u64 = (PORTAL_SET as u64) << 32;

/// The caption the client shows its own errors under.
pub const ERROR_CAPTION: &str = "AC2 Error";

/// What the client says when it shows notice `notice` of its own (from the strings it carries
/// beside its data files, not in them): for the notice a record that could not be stored shows,
/// that it could not save; `None` for a notice it has no line for.
// D2-09651, D2-09650
#[must_use]
pub fn notice_line(notice: u32) -> Option<&'static str> {
    match notice {
        auberean_dat::disk::download::SAVE_FAILED_NOTICE => Some(
            "Failed to save data. Are you out of disk space, or are your data files read-only?",
        ),
        _ => None,
    }
}

/// The iterations of the portal file no original data had, which a server gives what it adds for a
/// retail client: the script library that sends its calls in the clear. This client sends them so
/// already and knows that library's words itself, so it answers as if it held all of these and is
/// sent none of it. Sent, the library takes minutes, and at every login, since nothing a download
/// stores here outlasts the session: CD2-lists-iterations-it-need-not-hold.
pub const SERVER_LIBRARY_ITERATIONS: core::ops::Range<i32> = 9_000..10_000;

/// Who the data cache's requests tell, each asker by its own number: two sharing one would be told
/// each other's batches.
pub mod listeners {
    /// An object's creation.
    pub const CREATION: u32 = u32::MAX;
    /// The interface's text elements' string downloads (each download's number is the word they
    /// are told with).
    pub const STRINGS: u32 = u32::MAX - 1;
    /// The scene's cells about the player.
    pub const CELLS: u32 = u32::MAX - 2;
    /// Every one.
    pub const ALL: [u32; 3] = [CREATION, STRINGS, CELLS];
}

/// The runtime's side of the data download.
pub struct DataDownload {
    pub download: Download,
    /// Seconds since 1970, what a stored record is dated with.
    clock: Arc<AtomicU32>,
    /// Seconds since 1970 when the login started.
    start: u32,
    /// The connecting screen's data events (event, value) this session, oldest first.
    pub events: Vec<(u32, u32)>,
    /// The notice a failed store showed, which stops the client.
    pub fatal: Option<u32>,
    /// The data cache's requests.
    pub requests: Requests,
    /// What the requests' askers were told, oldest first.
    pub told: Vec<Event>,
    /// The client's data files, and its one data cache over them.
    files: DataFiles,
    cache: DataCache,
    /// Requests for records to send.
    asks: Vec<ClientMessage>,
    /// The interface's text elements' string downloads over the cache.
    strings: StringDownloads,
    /// Objects being made: each creation's batch, the object and the records it asked for.
    creating: HashMap<Batch, (u64, Vec<QualifiedId>)>,
    /// The creations whose records have all finished, in the order they did, for the objects'
    /// upkeep to make.
    finished: Vec<Batch>,
    /// The records each object was made from, held while it lives.
    held: HashMap<u64, Vec<AnyObj>>,
}

impl core::fmt::Debug for DataDownload {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DataDownload")
            .field("download", &self.download)
            .field("events", &self.events)
            .field("fatal", &self.fatal)
            .field("requests", &self.requests)
            .finish_non_exhaustive()
    }
}

impl Default for DataDownload {
    fn default() -> Self {
        let clock = Arc::new(AtomicU32::new(0));
        Self {
            download: Download::new(),
            cache: DataCache::new(Vec::new(), clock.clone()),
            clock,
            start: 0,
            events: Vec::new(),
            fatal: None,
            requests: {
                let mut r = Requests::new();
                r.server = true;
                r
            },
            told: Vec::new(),
            files: DataFiles::default(),
            asks: Vec::new(),
            strings: StringDownloads::default(),
            creating: HashMap::new(),
            finished: Vec::new(),
            held: HashMap::new(),
        }
    }
}

/// A set of data versions in the protocol's form.
fn iteration_set(set: &mut VersionSet) -> IterationSet {
    let mut w = Writer::new();
    set.pack(&mut w);
    let bytes = w.finish();
    IterationSet::read(&mut Reader::new(&bytes)).unwrap_or_default()
}

impl DataDownload {
    /// A download for a login started at `start` (seconds since 1970).
    #[must_use]
    pub fn new(start: u32) -> Self {
        let d = Self {
            start,
            ..Self::default()
        };
        d.clock.store(start, Ordering::Relaxed);
        d
    }

    /// The client's clock, `now` seconds into the session: what stored records are dated with.
    pub fn set_time(&self, now: f64) {
        let since = dereth_primitives::num::to_i32_f64(now.max(0.0)).cast_unsigned();
        self.clock
            .store(self.start.wrapping_add(since), Ordering::Relaxed);
    }

    /// The client's files as the download opens them (made at the first need); `None` for a
    /// client without data files.
    #[must_use]
    pub fn stores(&self) -> Option<&StoreSet> {
        (!self.cache.is_empty()).then(|| self.cache.stores())
    }

    /// The client's one data cache, shared with whatever else reads records.
    #[must_use]
    pub fn cache(&self) -> &DataCache {
        &self.cache
    }

    /// The interface's text elements' string downloads over the cache (clones share them).
    #[must_use]
    pub const fn strings(&self) -> &StringDownloads {
        &self.strings
    }

    /// A message's effect, when it is the download's: the download told, and what it asks sent
    /// pushed onto `send`, in order. Whether the effect was the download's.
    // D2-05138, D2-05156, D2-12846
    pub fn on_effect(
        &mut self,
        fx: &Effect,
        files: &DataFiles,
        send: &mut Vec<ClientMessage>,
        states: Option<&dyn StateRecords>,
    ) -> bool {
        if !matches!(
            fx,
            Effect::DataQuery
                | Effect::DownloadBegins(_)
                | Effect::DataRecord(_)
                | Effect::DataRefused(_)
                | Effect::DownloadWaiting(_)
                | Effect::DownloadEnds
        ) {
            return false;
        }
        self.open(files);
        let Self {
            download,
            requests,
            cache,
            ..
        } = self;
        let stores = cache.stores();
        match fx {
            Effect::DataQuery => {
                if let Some(lists) = download.on_server_interrogation(stores) {
                    let held = lists
                        .into_iter()
                        .map(|(dat_file, mut set)| {
                            if dat_file == PORTAL_FILE {
                                SERVER_LIBRARY_ITERATIONS.for_each(|v| set.add(v));
                            }
                            DataFileIterations {
                                dat_file,
                                iterations: iteration_set(&mut set),
                            }
                        })
                        .collect();
                    send.push(ClientMessage::DataHeld(DataHeld {
                        // D2-12612
                        language: files.language,
                        files: held,
                    }));
                }
            }
            Effect::DownloadBegins(b) => {
                let parts = b
                    .files
                    .iter()
                    .map(|m| {
                        MissingIteration::from_wire(m.dat_file, m.iteration, &m.download, &m.purge)
                    })
                    .collect();
                download.on_begin_ddd(stores, b.bytes, parts);
            }
            Effect::DataRecord(r) => {
                let data = cache_pack(r.size, &r.bytes);
                let length = u32::try_from(r.bytes.len() + 4).unwrap_or(u32::MAX);
                let q = QualifiedId {
                    id: r.id,
                    kind: r.kind,
                };
                let stored = download.on_data(stores, r.dat_file, q, length, data.as_deref());
                // Stored, the record goes to a request waiting for it (a failed store has stopped
                // the client).
                if stored == Some(true) && requests.get_state(q).is_some() {
                    let src = Sources {
                        states,
                        ..Sources::new(cache.database(), stores)
                    };
                    requests.on_saved(src, q, data);
                }
            }
            Effect::DataRefused(r) => download.on_refused(QualifiedId {
                id: r.id,
                kind: r.kind,
            }),
            Effect::DownloadWaiting(place) => download.on_waiting(*place),
            Effect::DownloadEnds => download.on_end_ddd(),
            _ => {}
        }
        self.drain(send);
        self.tell(send);
        true
    }

    /// What the download asks done: the screen's events kept, messages sent; a record the
    /// server could not send has no request waiting for it here.
    fn drain(&mut self, send: &mut Vec<ClientMessage>) {
        for o in std::mem::take(&mut self.download.out) {
            match o {
                Output::Notice { event, value } => self.events.push((event, value)),
                // D2-12847
                Output::SendEnd => send.push(ClientMessage::DownloadEnds(Empty)),
                Output::Request(q) => send.push(ClientMessage::DataRequest(DataRequest {
                    kind: q.kind,
                    id: q.id,
                })),
                Output::Refused(q) => {
                    if self.requests.get_state(q).is_some() {
                        let src = Sources::new(self.cache.database(), self.cache.stores());
                        self.requests.on_refused(src, q);
                    }
                }
                Output::Fatal(notice) => self.fatal = Some(notice),
            }
        }
    }
}

impl DataDownload {
    /// The download and the cache over the client's data files.
    #[must_use]
    pub fn over(mut self, files: &DataFiles) -> Self {
        self.files = files.clone();
        self.cache = DataCache::new(files.files.clone(), self.clock.clone());
        self.strings.set_cache(&self.cache);
        self
    }

    /// A download not yet given its files takes the ones it is first handed.
    fn open(&mut self, files: &DataFiles) {
        if self.cache.is_empty() && !files.files.is_empty() {
            *self = std::mem::take(self).over(files);
        }
    }

    /// The cache reset: the download back to idle, every batch and kept get let go.
    // D2-05083
    pub fn reset_cache(&mut self) {
        self.download.reset_cache();
        self.requests.flush_pending_requests();
    }

    /// Run `f` on the cache's requests with the files and the records' caches, and `states` to
    /// name a saved state's records; then take what it asks sent and what its askers are told.
    fn with_requests<T>(
        &mut self,
        states: Option<&dyn StateRecords>,
        f: impl FnOnce(&mut Requests, Sources<'_>) -> T,
    ) -> T {
        let src = Sources {
            states,
            ..Sources::new(self.cache.database(), self.cache.stores())
        };
        let v = f(&mut self.requests, src);
        let mut asks = std::mem::take(&mut self.asks);
        self.tell(&mut asks);
        self.asks = asks;
        v
    }

    /// Ask for a record without waiting, for the gathering `gather` (which of the records it
    /// names are asked for too); `listener` is told of it in [`DataDownload::told`]. The batch
    /// it is told in.
    pub fn get(&mut self, gather: u32, q: QualifiedId, listener: Option<Listener>) -> Batch {
        self.with_requests(None, |r, src| r.async_get(src, gather, q, listener))
    }

    /// Ask for several records in one batch, each for the gathering `gather`.
    pub fn get_many(
        &mut self,
        gather: u32,
        ids: &[QualifiedId],
        listener: Option<Listener>,
    ) -> Batch {
        self.get_many_named(gather, ids, listener, None)
    }

    /// [`DataDownload::get_many`], with `states` to name a saved state's records.
    pub fn get_many_named(
        &mut self,
        gather: u32,
        ids: &[QualifiedId],
        listener: Option<Listener>,
        states: Option<&dyn StateRecords>,
    ) -> Batch {
        self.with_requests(states, |r, src| {
            r.async_get_many(src, gather, ids, listener)
        })
    }

    /// A record now, from memory, a request's or the files (none when it is in none of them).
    pub fn get_now(
        &mut self,
        gather: u32,
        q: QualifiedId,
    ) -> Option<auberean_assets::cache::AnyObj> {
        self.with_requests(None, |r, src| r.blocking_get(src, gather, q))
    }

    /// Let a batch go.
    pub fn release(&mut self, batch: Batch) {
        self.requests.release_batch(batch);
    }

    /// Object `object`'s creation asked for `ids` in `batch`.
    pub(crate) fn creating(&mut self, batch: Batch, object: u64, ids: Vec<QualifiedId>) {
        self.creating.insert(batch, (object, ids));
    }

    /// The creation in `batch` has all its records: the object is made from them. Each record
    /// that came (from memory, or read by its request) goes into its type's cache, and so does
    /// each record it names that the creation read for it, in turn; the object holds them while
    /// it lives, in place of what an earlier incarnation held.
    pub(crate) fn made(&mut self, batch: Batch) {
        let Some((object, ids)) = self.creating.remove(&batch) else {
            return;
        };
        if self.cache.is_empty() {
            return;
        }
        let src = Sources::new(self.cache.database(), self.cache.stores());
        let held = take_made_from(&mut self.requests, src, &ids);
        self.held.insert(object, held);
    }

    /// Object `object` is gone: the records it was made from are let go (onto their caches'
    /// free lists), and a creation of it still under way makes nothing.
    pub fn object_gone(&mut self, object: u64) {
        self.held.remove(&object);
        self.creating.retain(|_, (o, _)| *o != object);
    }

    /// Every object is gone (the world was left): what they were made from is let go, and so are
    /// the creations under way.
    pub fn objects_gone(&mut self) {
        self.held.clear();
        self.clear_creations();
    }

    /// Every creation under way is given up: its batch let go, and it makes nothing.
    // D2-05666
    pub fn clear_creations(&mut self) {
        let mut batches: Vec<Batch> = self.creating.keys().copied().collect();
        batches.extend(self.finished.iter().copied());
        batches.sort_unstable();
        batches.dedup();
        for b in batches {
            self.requests.release_batch(b);
        }
        self.creating.clear();
        self.finished.clear();
    }

    /// Whether object `object` is being made: its creation's records have not all finished, or
    /// the objects' upkeep has not made it from them yet.
    // D2-05668
    #[must_use]
    pub fn is_object_being_created(&self, object: u64) -> bool {
        self.creating.values().any(|(o, _)| *o == object)
    }

    /// The objects' upkeep: each creation whose records have all finished makes its object, in the
    /// order they finished, and lets its batch go.
    pub fn finish_creations(&mut self) {
        for b in std::mem::take(&mut self.finished) {
            crate::creation::finish(self, b);
        }
    }

    /// How many records object `object` holds, those it was made from.
    #[must_use]
    pub fn held_by(&self, object: u64) -> usize {
        self.held.get(&object).map_or(0, Vec::len)
    }

    /// Give the cache its time, `now` seconds into the session: the worker runs, the requests
    /// it did finish, and their askers are told (`states` names a saved state's records). What is
    /// to be sent.
    pub fn use_time(&mut self, now: f64, states: Option<&dyn StateRecords>) -> Vec<ClientMessage> {
        strings::ask(self);
        if self.cache.is_open() {
            self.with_requests(states, |r, src| r.use_time(src, now));
        }
        // What the string downloads were told to ask for next.
        strings::ask(self);
        std::mem::take(&mut self.asks)
    }

    /// What the cache asks: records of the server sent as `DatRequest` (`0x10034`), the rest kept
    /// for the askers.
    fn tell(&mut self, send: &mut Vec<ClientMessage>) {
        let mut created = Vec::new();
        let mut texts = Vec::new();
        for e in std::mem::take(&mut self.requests.out) {
            match e {
                e if strings::is_theirs(&e) => texts.push(e),
                Event::Ask(q) => send.push(ClientMessage::DataRequest(DataRequest {
                    kind: q.kind,
                    id: q.id,
                })),
                // An object's creation is kept for the objects' upkeep once its records have all
                // finished.
                Event::BatchDone { to, batch, .. } if to == crate::creation::CREATION.to => {
                    // D2-05680
                    created.push(batch);
                }
                Event::Finished { to, .. } if to == crate::creation::CREATION.to => {}
                other => self.told.push(other),
            }
        }
        strings::told(self, texts);
        self.finished.extend(created);
    }
}

/// The records an object is made from, taken into their caches: `ids` and, in turn, every record
/// each named as its request gathered them, each only when its request finished with it (a
/// record refused, or not asked for because it was in memory already, is not taken here).
fn take_made_from(requests: &mut Requests, src: Sources<'_>, ids: &[QualifiedId]) -> Vec<AnyObj> {
    let gather = crate::creation::GATHER_CREATION;
    let mut seen: HashSet<QualifiedId> = ids.iter().copied().collect();
    let mut next: Vec<QualifiedId> = ids.to_vec();
    let mut held = Vec::new();
    while let Some(q) = next.pop() {
        let came = matches!(
            requests.get_state(q),
            Some((Outcome::Success | Outcome::Partial, _))
        );
        let Some(o) = came
            .then(|| requests.blocking_get(src, gather, q))
            .flatten()
        else {
            continue;
        };
        let named = requests.named_by(q);
        next.extend(named.into_iter().filter(|n| seen.insert(*n)));
        held.push(o);
    }
    held
}
