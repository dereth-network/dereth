//! The data cache's requests: records asked for without waiting, each loaded once however many
//! ask for it, the records each one names asked for in turn, and each asker told when its records
//! are in.
//!
//! **A get** ([`Requests::async_get`]) makes one request per record, found again by its type and
//! id for as long as it is kept ([`Requests::pending`]): a second asker shares the first's
//! request, and one already finished answers at once. A request not yet under way is kept and
//! started: a record in memory is taken from its type's cache; one in the data files goes to the
//! worker to be read; one in neither is asked of the server ([`Event::Ask`]), whose record
//! arriving ([`Requests::on_saved`]) or refusal ([`Requests::on_refused`]) finishes it; one the
//! server cannot be asked for fails. A record read is decoded, and the records it names (by the
//! gathering the get was made for) are asked for the same way: the request finishes when they all
//! have, a partial success when any of them failed.
//!
//! **Purges and saves** ([`Requests::async_purge`], [`Requests::async_save`]) go to the worker,
//! which removes or stores the record in its data file.
//!
//! **Askers** are told through a [`Listener`]: each record of theirs as it finishes
//! ([`Event::Finished`]), and, the next time the cache is given time, that all of a batch has
//! ([`Event::BatchDone`], with the results summed up). An asker lets its batch go
//! ([`Requests::release_batch`]); the requests nobody waits on are then forgotten.
//!
//! **An asker that grows its requests** ([`Requests::set_hook`]) hears each of its records finish
//! the moment it does, before the request counts as done, and may add records to that request:
//! the request then waits on those too, and finishes again (once more for the asker) when they
//! are in. This is how the client gets the world's cells a ring of landblocks at a time under one
//! request. A host may grow a get itself too, found by its record or by its batch
//! ([`Requests::add_to_get`], [`Requests::add_to_batch_get`]).
//!
//! **A saved script state** names its records only through the scripts, which the cache cannot run
//! itself: a host that runs them answers for it ([`StateRecords`], in [`Sources::states`]), and the
//! records it names are asked for the same way, so every state they name is asked in turn. Without
//! one, a state names none.
//!
//! **No threads and no I/O.** The client's worker thread reads and writes the data files beside
//! the game and hands back what it did; here the worker is run in place each time the cache is
//! given time ([`Requests::use_time`]): every request queued runs, in order, then every one done
//! is finished, in order, as the client finishes them on its next frame. The files are the
//! [`StoreSet`] and the records' caches the [`Database`] given to each call ([`Sources`]); asking
//! the server is an [`Event`] for the host to send. What can differ from the threaded client is
//! only when the worker's part runs: in the client it may run part way through a frame, here at
//! its end; a client given less than its 25 milliseconds a frame finishes some requests a frame
//! later than here.
//!
//! Reference counts are kept as the client keeps them: a request or a batch is let go when the
//! last holder lets go of it, and only then does what it held go.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use auberean_dat::disk::StoreSet;
use auberean_dat::{QualifiedId, QualifiedIds, RecordType};

use crate::cache::{self, AnyObj, Held, Key};
use crate::database::{gather_sub_ids, Database};
use crate::record::Record;
use crate::script::ScriptState;

/// The gathering that asks for none of a record's named records.
pub const GATHER_NONE: u32 = 0x4000_0006;

/// How a request ended, and how a batch's ended summed up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Outcome {
    #[default]
    Unknown = 0,
    Success = 1,
    /// The record, but not every record it names (or, for a batch, not every record).
    Partial = 2,
    Failure = 3,
}

/// What a request does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Get = 0,
    Purge = 1,
    Save = 2,
}

/// A batch of requests whose asker is told about them together; [`Batch::NONE`] when nobody is.
// D2-17935
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Batch(pub u32);

impl Batch {
    pub const NONE: Self = Self(0);
}

/// Who is told about a batch (the host's own number for it) and the word it gave to be told
/// with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Listener {
    pub to: u32,
    pub user: u32,
}

/// What the host must do or pass on, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Ask the server for this record.
    Ask(QualifiedId),
    /// A record of a batch finished (`times`: how often it has for that batch).
    Finished {
        to: u32,
        record: QualifiedId,
        outcome: Outcome,
        user: u32,
        times: u32,
    },
    /// Every request of a batch finished.
    BatchDone {
        to: u32,
        batch: Batch,
        outcome: Outcome,
        user: u32,
    },
}

/// What an asker that grows its requests does as each of its records finishes: the records to
/// add to that record's request, each set with the gathering it is got for (none to add nothing).
pub trait OnFinished: Send + Sync {
    /// Record `record` of the asker's batch with word `user` finished with `outcome`, the
    /// `times`th time for that batch. `db` holds what the cache has loaded.
    fn finished(
        &mut self,
        db: &Database,
        record: QualifiedId,
        outcome: Outcome,
        user: u32,
        times: u32,
    ) -> Vec<(u32, Vec<QualifiedId>)>;
}

/// What names the records a saved script state needs: the scripts, run by their host on the state
/// as it was loaded.
pub trait StateRecords {
    /// The records state `id` (loaded as `state`) names when it is got for gathering `gather`,
    /// added to `out`.
    fn state_records(&self, id: u32, state: &ScriptState, gather: u32, out: &mut QualifiedIds);
}

/// The data files and the records' caches a call works on, and what names a saved state's
/// records (none: a state names none).
#[derive(Clone, Copy)]
pub struct Sources<'a> {
    pub db: &'a Database,
    pub files: &'a StoreSet,
    pub states: Option<&'a dyn StateRecords>,
}

impl<'a> Sources<'a> {
    /// The files and caches, with nothing to name a saved state's records.
    #[must_use]
    pub const fn new(db: &'a Database, files: &'a StoreSet) -> Self {
        Self {
            db,
            files,
            states: None,
        }
    }
}

impl core::fmt::Debug for Sources<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Sources")
            .field("db", self.db)
            .field("files", self.files)
            .field("states", &self.states.is_some())
            .finish()
    }
}

/// The request is kept (found again by its record).
const HASHED: u32 = 1;
/// The request is under way.
const WORKING: u32 = 2;
/// The record it holds is in its type's cache.
const IN_CACHE: u32 = 4;

/// A record a request holds: one in its type's cache, or one it decoded itself and holds alone.
enum Loaded {
    InCache(AnyObj),
    Fresh { kind: u32, key: Key, value: Held },
}

struct Request {
    refs: i32,
    outcome: Outcome,
    op: Op,
    q: QualifiedId,
    /// Each batch told about it, and how often it has finished for that batch.
    callbacks: Vec<(usize, u32)>,
    obj: Option<Loaded>,
    // A get's:
    gather: u32,
    waiting_for_me: Vec<usize>,
    im_waiting_for: Vec<usize>,
    /// The record's bytes as read or sent (a save's: the bytes to store).
    buf: Option<Vec<u8>>,
    remaining: i32,
    /// Whether the record's type has a cache (a decoder, here).
    has_cache: bool,
    flags: u32,
    // A save's:
    dat_file: u64,
}

impl Request {
    /// A request of `op` for `q`, with one holder.
    // D2-12783
    fn new(op: Op, q: QualifiedId) -> Self {
        Self {
            refs: 1,
            outcome: Outcome::Unknown,
            op,
            q,
            callbacks: Vec::new(),
            obj: None,
            gather: 0,
            waiting_for_me: Vec::new(),
            im_waiting_for: Vec::new(),
            buf: None,
            remaining: 0,
            has_cache: false,
            flags: 0,
            dat_file: 0,
        }
    }

    /// Whether everything it waits on has finished (a purge or a save waits on nothing).
    // D2-12779
    fn all_done(&self) -> bool {
        self.op != Op::Get || self.remaining == 0
    }
}

struct Handler {
    refs: i32,
    pending: i32,
    listener: Option<Listener>,
    accumulated: Outcome,
    top_level: Vec<usize>,
    batch: Batch,
}

/// The cache's requests.
// D2-05137
pub struct Requests {
    reqs: Vec<Option<Request>>,
    free_reqs: Vec<usize>,
    handlers: Vec<Option<Handler>>,
    free_handlers: Vec<usize>,
    /// The gets kept, by record.
    pending_gets: BTreeMap<QualifiedId, usize>,
    /// The batches not let go, by number.
    busy: BTreeMap<Batch, usize>,
    next_batch: u32,
    /// Batches all of whose requests finished, to be told the next time the cache is given time.
    done: VecDeque<usize>,
    /// The worker's queue, and what it has done.
    jobs: VecDeque<usize>,
    replies: VecDeque<usize>,
    /// Whether a worker runs the queue (the client's); else each request runs as it is made.
    threaded: bool,
    stopped: bool,
    /// Whether the server can be asked for a record (the client is connected).
    pub server: bool,
    pub out: Vec<Event>,
    /// The askers that grow their requests, by the number they are told by.
    hooks: BTreeMap<u32, Box<dyn OnFinished>>,
}

impl Default for Requests {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for Requests {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Requests")
            .field("live", &self.live())
            .field("pending", &self.pending_gets.len())
            .field("batches", &self.busy.len())
            .field("jobs", &self.jobs.len())
            .finish_non_exhaustive()
    }
}

impl Requests {
    /// No request yet; a worker runs the queue; the server is not asked.
    // D2-12638, D2-12825
    #[must_use]
    pub fn new() -> Self {
        Self {
            reqs: Vec::new(),
            free_reqs: Vec::new(),
            handlers: Vec::new(),
            free_handlers: Vec::new(),
            pending_gets: BTreeMap::new(),
            busy: BTreeMap::new(),
            next_batch: 1,
            done: VecDeque::new(),
            jobs: VecDeque::new(),
            replies: VecDeque::new(),
            threaded: true,
            stopped: false,
            server: false,
            out: Vec::new(),
            hooks: BTreeMap::new(),
        }
    }

    /// The asker told by `to` hears its records finish through `hook` too, and grows their
    /// requests by what it answers ([`OnFinished`]); `None` stops it. An asker without one is
    /// only told ([`Event::Finished`]).
    pub fn set_hook(&mut self, to: u32, hook: Option<Box<dyn OnFinished>>) {
        match hook {
            Some(h) => {
                self.hooks.insert(to, h);
            }
            None => {
                self.hooks.remove(&to);
            }
        }
    }

    /// Without a worker: every request carried out as it is made, the record got, removed or
    /// stored before the call returns.
    #[must_use]
    pub fn in_place() -> Self {
        Self {
            threaded: false,
            ..Self::new()
        }
    }

    // ------------------------------------------------------------------ what the host sees

    /// How many requests are alive.
    #[must_use]
    pub fn live(&self) -> usize {
        self.reqs.iter().flatten().count()
    }

    /// How many batches are alive (let go or not).
    #[must_use]
    pub fn live_batches(&self) -> usize {
        self.handlers.iter().flatten().count()
    }

    /// The records whose gets are kept.
    #[must_use]
    pub fn pending(&self) -> Vec<QualifiedId> {
        self.pending_gets.keys().copied().collect()
    }

    /// How a kept get stands: its outcome so far, and how many records it still waits on (itself
    /// included).
    #[must_use]
    pub fn get_state(&self, q: QualifiedId) -> Option<(Outcome, i32)> {
        let r = self.req(*self.pending_gets.get(&q)?);
        Some((r.outcome, r.remaining))
    }

    /// The records a kept get named, as it gathered them, each got in turn (none when no get of
    /// `q` is kept, or it named none).
    #[must_use]
    pub fn named_by(&self, q: QualifiedId) -> Vec<QualifiedId> {
        self.pending_gets.get(&q).map_or_else(Vec::new, |&i| {
            self.req(i)
                .im_waiting_for
                .iter()
                .map(|&c| self.req(c).q)
                .collect()
        })
    }

    /// How many requests wait for the worker.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.jobs.len()
    }

    // ------------------------------------------------------------------ the slabs

    fn req(&self, i: usize) -> &Request {
        self.reqs[i].as_ref().expect("a live request")
    }

    fn req_mut(&mut self, i: usize) -> &mut Request {
        self.reqs[i].as_mut().expect("a live request")
    }

    fn add_req(&mut self, r: Request) -> usize {
        if let Some(i) = self.free_reqs.pop() {
            self.reqs[i] = Some(r);
            i
        } else {
            self.reqs.push(Some(r));
            self.reqs.len() - 1
        }
    }

    fn hnd(&self, h: usize) -> &Handler {
        self.handlers[h].as_ref().expect("a live batch")
    }

    fn hnd_mut(&mut self, h: usize) -> &mut Handler {
        self.handlers[h].as_mut().expect("a live batch")
    }

    fn retain(&mut self, i: usize) {
        self.req_mut(i).refs += 1;
    }

    /// One holder fewer; the last lets the request go, and with it its holds on the requests it
    /// waits on and that wait on it, its bytes and its record. (A record it decoded itself and
    /// holds alone goes with it, never into the cache.)
    // D2-12775, D2-12778
    fn release(&mut self, i: usize) {
        let r = self.req_mut(i);
        r.refs -= 1;
        if r.refs != 0 {
            return;
        }
        let r = self.reqs[i].take().expect("a live request");
        for w in &r.im_waiting_for {
            self.release(*w);
        }
        for w in &r.waiting_for_me {
            self.release(*w);
        }
        drop(r);
        self.free_reqs.push(i);
    }

    fn retain_h(&mut self, h: usize) {
        self.hnd_mut(h).refs += 1;
    }

    /// One holder of a batch fewer; the last lets it go, off every request it was told about,
    /// and lets go of its own requests.
    // D2-12787
    fn release_h(&mut self, h: usize) {
        let b = self.hnd_mut(h);
        b.refs -= 1;
        if b.refs != 0 {
            return;
        }
        let b = self.handlers[h].take().expect("a live batch");
        for r in &b.top_level {
            // Off the request (its hold on this batch is already gone with the batch).
            if let Some(req) = self.reqs[*r].as_mut() {
                if let Some(k) = req.callbacks.iter().position(|(c, _)| *c == h) {
                    req.callbacks.swap_remove(k);
                }
            }
            self.release(*r);
        }
        self.free_handlers.push(h);
    }

    // ------------------------------------------------------------------ batches

    /// A new batch for `listener`, waiting on `n` requests (none without a listener); the caller
    /// holds it, and so do the batches not let go.
    // D2-12809
    fn allocate_callback(&mut self, listener: Option<Listener>, n: i32) -> Option<usize> {
        let listener = listener?;
        if self.next_batch == 0 {
            self.next_batch = 1;
        }
        let batch = Batch(self.next_batch);
        self.next_batch = self.next_batch.wrapping_add(1);
        let b = Handler {
            refs: 1,
            pending: n,
            listener: Some(listener),
            accumulated: Outcome::Unknown,
            top_level: Vec::new(),
            batch,
        };
        let h = if let Some(h) = self.free_handlers.pop() {
            self.handlers[h] = Some(b);
            h
        } else {
            self.handlers.push(Some(b));
            self.handlers.len() - 1
        };
        self.retain_h(h);
        self.busy.insert(batch, h);
        Some(h)
    }

    /// Tell batch `h` about request `i` (each holds the other).
    // D2-12806
    fn add_top_level_request(&mut self, h: usize, i: usize) {
        self.retain(i);
        self.hnd_mut(h).top_level.push(i);
        self.add_callback(i, h);
    }

    /// Batch `h` is told when request `i` finishes.
    // D2-12798
    fn add_callback(&mut self, i: usize, h: usize) {
        self.retain_h(h);
        self.req_mut(i).callbacks.push((h, 0));
    }

    /// Batch `h` is no longer told about request `i`; whether it was.
    // D2-12785
    fn remove_callback(&mut self, i: usize, h: usize) -> bool {
        let Some(k) = self.req(i).callbacks.iter().position(|(c, _)| *c == h) else {
            return false;
        };
        self.req_mut(i).callbacks.swap_remove(k);
        self.release_h(h);
        true
    }

    /// No batch is told about request `i` any more.
    // D2-12776
    fn remove_all_callbacks(&mut self, i: usize) {
        let cs = std::mem::take(&mut self.req_mut(i).callbacks);
        for (h, _) in cs {
            self.release_h(h);
        }
    }

    /// A batch let go: off each of its requests, which it lets go, and nobody is told of it any
    /// more.
    // D2-12786
    fn on_context_released(&mut self, h: usize) {
        let top = std::mem::take(&mut self.hnd_mut(h).top_level);
        for r in top {
            self.remove_callback(r, h);
            self.release(r);
        }
        let b = self.hnd_mut(h);
        b.listener = None;
        b.batch = Batch::NONE;
    }

    /// The asker lets its batch go: its gets nobody else waits on are forgotten (with the gets
    /// they wait on that nobody else does), and it is told nothing more.
    // D2-12802
    pub fn release_batch(&mut self, batch: Batch) {
        let Some(h) = self.busy.remove(&batch) else {
            return;
        };
        let top = self.hnd(h).top_level.clone();
        for r in top {
            if self.req(r).op == Op::Get {
                self.unhash_pending_get(r, None);
            }
        }
        self.on_context_released(h);
        self.release_h(h);
    }

    /// Tell each batch done its asker, in the order they finished, when it still has one.
    // D2-12789
    pub fn tell_done_batches(&mut self) {
        while let Some(h) = self.done.pop_front() {
            let b = self.hnd(h);
            if let Some(l) = b.listener {
                if b.batch != Batch::NONE {
                    self.out.push(Event::BatchDone {
                        to: l.to,
                        batch: b.batch,
                        outcome: b.accumulated,
                        user: l.user,
                    });
                }
            }
            self.release_h(h);
        }
    }

    /// Request `i` finished: each batch told about a get is told of it; once everything it waits
    /// on has finished, each batch counts it and sums its outcome into the batch's (a success and
    /// a failure make a partial success), a batch with nothing left to wait on is done, and no
    /// batch is told about the request any more.
    // D2-12803
    fn notify_callback(&mut self, src: Sources<'_>, i: usize) {
        let (op, q, outcome) = {
            let r = self.req(i);
            (r.op, r.q, r.outcome)
        };
        if op == Op::Get {
            // An asker that grows the request may finish it (and so take it off its batches)
            // before the next is told: the list is read afresh each time round.
            let mut k = 0;
            while k < self.req(i).callbacks.len() {
                let (h, times) = {
                    let c = &mut self.req_mut(i).callbacks[k];
                    c.1 += 1;
                    *c
                };
                k += 1;
                let Some(l) = self.hnd(h).listener else {
                    continue;
                };
                self.out.push(Event::Finished {
                    to: l.to,
                    record: q,
                    outcome,
                    user: l.user,
                    times,
                });
                if let Some(mut hook) = self.hooks.remove(&l.to) {
                    let more = hook.finished(src.db, q, outcome, l.user, times);
                    self.hooks.entry(l.to).or_insert(hook);
                    for (gather, ids) in more {
                        if !ids.is_empty() {
                            self.add_to_async_get(src, gather, &ids, i);
                        }
                    }
                }
            }
        }
        if !self.req(i).all_done() {
            return;
        }
        for k in 0..self.req(i).callbacks.len() {
            let h = self.req(i).callbacks[k].0;
            let b = self.hnd_mut(h);
            b.pending -= 1;
            b.accumulated = sum(b.accumulated, outcome);
            if b.pending == 0 {
                let batch = b.batch;
                if self.busy.contains_key(&batch) {
                    self.retain_h(h);
                    self.done.push_back(h);
                }
            }
        }
        self.remove_all_callbacks(i);
    }

    // ------------------------------------------------------------------ gets

    /// The get for `q`: the one kept, given another holder, or a new one waiting on itself, for
    /// the gathering `gather`.
    // D2-12799
    fn form_get_request(&mut self, gather: u32, q: QualifiedId) -> usize {
        if let Some(&i) = self.pending_gets.get(&q) {
            self.retain(i);
            return i;
        }
        let mut r = Request::new(Op::Get, q);
        r.has_cache = Record::has_decoder(RecordType(q.kind));
        r.gather = gather;
        r.remaining = 1;
        self.add_req(r)
    }

    /// Keep get `i` (found again by its record) and start it; one kept already is left be.
    // D2-12815
    fn hash_and_enqueue(&mut self, src: Sources<'_>, i: usize) {
        let q = self.req(i).q;
        if self.pending_gets.contains_key(&q) {
            return;
        }
        self.pending_gets.insert(q, i);
        self.retain(i);
        self.req_mut(i).flags |= HASHED | WORKING;
        self.enqueue(src, i);
    }

    /// Start a request: with a worker, a get is started here and a purge or save queued for the
    /// worker; without one, each is carried out at once.
    // D2-12831, D2-12761
    fn enqueue(&mut self, src: Sources<'_>, i: usize) {
        if self.threaded {
            if self.req(i).op == Op::Get {
                self.async_get_internal(src, i);
            } else {
                self.put_on_worker_queue(i);
            }
            return;
        }
        let (op, q, gather) = {
            let r = self.req(i);
            (r.op, r.q, r.gather)
        };
        if q.id == 0 {
            self.req_mut(i).outcome = Outcome::Failure;
            self.on_request_finished(src, i);
            return;
        }
        let ok = match op {
            Op::Get => {
                let o = self.blocking_get(src, gather, q);
                let found = o.is_some();
                self.adopt(src, i, o.map(Loaded::InCache));
                found
            }
            Op::Purge => src.files.purge_data(q),
            Op::Save => {
                let data = self.req(i).buf.clone().unwrap_or_default();
                src.files.save_data(q, &data)
            }
        };
        self.req_mut(i).outcome = if ok {
            Outcome::Success
        } else {
            Outcome::Failure
        };
        self.on_request_finished(src, i);
    }

    /// Queue a request for the worker (the queue holds it).
    // D2-12828
    fn put_on_worker_queue(&mut self, i: usize) {
        self.retain(i);
        self.jobs.push_back(i);
    }

    /// Start a get: a record in memory finishes it now; one on disk goes to the worker; one in
    /// neither is asked of the server, and waits; else it fails.
    // D2-12829
    fn async_get_internal(&mut self, src: Sources<'_>, i: usize) {
        let (q, has_cache) = {
            let r = self.req(i);
            (r.q, r.has_cache)
        };
        if let Some(o) = in_memory(src, q, has_cache) {
            self.req_mut(i).flags |= IN_CACHE;
            self.adopt(src, i, Some(Loaded::InCache(o)));
            self.req_mut(i).outcome = Outcome::Success;
            self.on_get_request_finished(src, i);
            return;
        }
        self.req_mut(i).flags &= !IN_CACHE;
        if has_cache && src.files.is_on_disk(q) {
            self.put_on_worker_queue(i);
            return;
        }
        if has_cache && self.ask_other_sources(q) {
            self.req_mut(i).flags &= !WORKING;
            return;
        }
        self.req_mut(i).outcome = Outcome::Failure;
        self.on_request_finished(src, i);
    }

    /// Ask the server for a record; whether it could be (the client asks when it is connected;
    /// the cache without a server never can).
    // D2-05111, D2-14693
    fn ask_other_sources(&mut self, q: QualifiedId) -> bool {
        if !self.server {
            return false;
        }
        self.out.push(Event::Ask(q));
        true
    }

    /// Request `i` holds `obj` now, letting go of what it held.
    // D2-12759
    fn adopt(&mut self, src: Sources<'_>, i: usize, obj: Option<Loaded>) {
        self.release_obj(src, i);
        self.req_mut(i).obj = obj;
    }

    /// Request `i` lets go of its record: a get's own decoded record goes into its type's cache
    /// first (a purge's or a save's, if it had one, simply goes).
    // D2-12760, D2-12777
    fn release_obj(&mut self, src: Sources<'_>, i: usize) {
        let r = self.req_mut(i);
        let get = r.op == Op::Get;
        let in_cache = r.flags & IN_CACHE != 0;
        match r.obj.take() {
            Some(Loaded::Fresh { kind, key, value }) if get && !in_cache => {
                drop(add_obj_to_cache(src, kind, key, value));
            }
            other => drop(other),
        }
    }

    /// A get finished its own record: one read is decoded (a record in memory is taken from its
    /// cache instead); the records it names are asked for unless it was got with them; then it is
    /// done when nothing else is left to wait on. A record not read, not decoded, or of a type
    /// with no cache fails.
    // D2-12816
    fn on_get_request_finished(&mut self, src: Sources<'_>, i: usize) {
        self.req_mut(i).flags &= !WORKING;
        if self.req(i).outcome == Outcome::Success {
            let (q, has_cache, gather) = {
                let r = self.req(i);
                (r.q, r.has_cache, r.gather)
            };
            let mut named = None;
            if let Some(o) = in_memory(src, q, has_cache) {
                self.release_obj(src, i);
                let r = self.req_mut(i);
                r.obj = None;
                let key = (o.id(), rust_type(q.kind));
                let loaded = cache::subs_loaded(src.db.caches(), q.kind, key);
                if !loaded && gather != GATHER_NONE {
                    named = o.value().map(|v| sub_ids(src, q, v, gather));
                }
                r.obj = Some(Loaded::InCache(o));
                r.flags |= IN_CACHE;
            } else {
                // A new record of the type, made room for, decoded from the bytes read.
                let fresh = if has_cache {
                    cache::get_free_obj(src.db.caches(), q.kind);
                    let bytes = self.req_mut(i).buf.take();
                    bytes.and_then(|b| decode(src, q, &b))
                } else {
                    None
                };
                self.release_obj(src, i);
                let r = self.req_mut(i);
                r.flags &= !IN_CACHE;
                r.buf = None;
                match fresh {
                    Some(value) => {
                        named = (gather != GATHER_NONE).then(|| sub_ids(src, q, &*value, gather));
                        r.obj = Some(Loaded::Fresh {
                            kind: q.kind,
                            key: (q.id, rust_type(q.kind)),
                            value,
                        });
                    }
                    None => {
                        r.obj = None;
                        r.outcome = Outcome::Failure;
                    }
                }
            }
            if let Some(ids) = named {
                self.add_to_async_get(src, gather, &ids, i);
            }
        }
        let r = self.req_mut(i);
        r.remaining -= 1;
        if r.remaining == 0 {
            self.on_async_get_finished(src, i);
        }
    }

    /// Get `parent` waits on the records `ids` too (itself excepted): each one's get is formed
    /// (and started when new); one finished already is counted at once (a failure making the
    /// parent's a partial success). With nothing left to wait on, the parent is done. Only a get
    /// waits on records (`false` for anything else).
    // D2-12807
    fn add_to_async_get(
        &mut self,
        src: Sources<'_>,
        _gather: u32,
        ids: &[QualifiedId],
        parent: usize,
    ) -> bool {
        if self.req(parent).op != Op::Get {
            return false;
        }
        let n = i32::try_from(ids.len()).unwrap_or(i32::MAX);
        self.req_mut(parent).remaining += n;
        for q in ids {
            let (own, gather) = {
                let p = self.req(parent);
                (p.q, p.gather)
            };
            if *q == own {
                self.req_mut(parent).remaining -= 1;
                continue;
            }
            let child = self.form_get_request(gather, *q);
            if child == parent {
                self.release(child);
                self.req_mut(parent).remaining -= 1;
                continue;
            }
            self.req_mut(parent).im_waiting_for.push(child);
            if self.req(child).all_done() {
                if self.req(child).outcome != Outcome::Success {
                    self.req_mut(parent).outcome = Outcome::Partial;
                }
                self.req_mut(parent).remaining -= 1;
                continue;
            }
            self.retain(parent);
            self.req_mut(child).waiting_for_me.push(parent);
            if self.req(child).flags & HASHED == 0 {
                self.hash_and_enqueue(src, child);
            }
        }
        if self.req(parent).remaining == 0 {
            self.on_async_get_finished(src, parent);
        }
        true
    }

    /// Get `i` is done (with every record it waits on): a success without a record fails; each
    /// get waiting on it counts it (its failure making theirs a partial success), and one with
    /// nothing left is done in turn; then its batches are told.
    // D2-12780
    fn on_async_get_finished(&mut self, src: Sources<'_>, i: usize) {
        {
            let r = self.req_mut(i);
            if r.outcome == Outcome::Success && r.obj.is_none() {
                r.outcome = Outcome::Failure;
            }
        }
        let mut k = 0;
        while k < self.req(i).waiting_for_me.len() {
            let w = self.req(i).waiting_for_me[k];
            let failed = self.req(i).outcome == Outcome::Failure;
            let left = {
                let wr = self.req_mut(w);
                wr.remaining -= 1;
                if failed {
                    wr.outcome = Outcome::Partial;
                }
                wr.remaining
            };
            if left == 0 {
                self.on_async_get_finished(src, w);
            }
            self.release(w);
            k += 1;
        }
        self.req_mut(i).waiting_for_me.clear();
        self.notify_callback(src, i);
    }

    /// Get `i` no longer has `waiter` waiting on it; kept, with nobody waiting and no batch told
    /// about it, it is forgotten, and so in turn are the gets it waits on.
    // D2-12801
    fn unhash_pending_get(&mut self, i: usize, waiter: Option<usize>) {
        if let Some(w) = waiter {
            let list = &mut self.req_mut(i).waiting_for_me;
            if let Some(k) = list.iter().position(|x| *x == w) {
                list.swap_remove(k);
            }
        }
        let r = self.req(i);
        if r.flags & HASHED == 0 || !r.waiting_for_me.is_empty() || !r.callbacks.is_empty() {
            return;
        }
        let q = r.q;
        self.req_mut(i).flags &= !HASHED;
        if self.pending_gets.get(&q) == Some(&i) {
            self.pending_gets.remove(&q);
            let mine = self.req(i).im_waiting_for.clone();
            for c in mine {
                self.unhash_pending_get(c, Some(i));
            }
            self.release(i);
        }
    }

    /// Grow the kept get of record `parent` by the records `ids` (got for the gathering
    /// `gather`), as an asker grows its request: the get waits on them too, and finishes again
    /// when they are in (told to the batches still told about it: none once it was done).
    /// `false` when no get of that record is kept.
    // D2-12807
    pub fn add_to_get(
        &mut self,
        src: Sources<'_>,
        gather: u32,
        ids: &[QualifiedId],
        parent: QualifiedId,
    ) -> bool {
        let Some(&i) = self.pending_gets.get(&parent) else {
            return false;
        };
        self.add_to_async_get(src, gather, ids, i)
    }

    /// [`Requests::add_to_get`] for the get of record `record` that batch `batch` asked for
    /// (kept or not). `false` when the batch has been let go or did not ask for that record.
    // D2-12807
    pub fn add_to_batch_get(
        &mut self,
        src: Sources<'_>,
        gather: u32,
        ids: &[QualifiedId],
        batch: Batch,
        record: QualifiedId,
    ) -> bool {
        let Some(&h) = self.busy.get(&batch) else {
            return false;
        };
        let Some(i) = self
            .hnd(h)
            .top_level
            .iter()
            .copied()
            .find(|&r| self.req(r).q == record)
        else {
            return false;
        };
        self.add_to_async_get(src, gather, ids, i)
    }

    /// Ask for a record without waiting, for the gathering `gather` (which of the records it
    /// names are asked for too; [`GATHER_NONE`] for none); `listener` is told of it. The batch
    /// it is told in.
    // D2-12810
    pub fn async_get(
        &mut self,
        src: Sources<'_>,
        gather: u32,
        q: QualifiedId,
        listener: Option<Listener>,
    ) -> Batch {
        let i = self.form_get_request(gather, q);
        let mut batch = Batch::NONE;
        let h = if listener.is_some() {
            let h = self.allocate_callback(listener, 1);
            if let Some(h) = h {
                self.add_top_level_request(h, i);
                batch = self.hnd(h).batch;
            }
            h
        } else {
            None
        };
        if self.req(i).all_done() {
            self.notify_callback(src, i);
        } else if self.req(i).flags & HASHED == 0 {
            self.hash_and_enqueue(src, i);
        }
        if let Some(h) = h {
            self.release_h(h);
        }
        self.release(i);
        batch
    }

    /// Ask for several records in one batch ([`Batch::NONE`] for none, or without a listener).
    // D2-12811
    pub fn async_get_many(
        &mut self,
        src: Sources<'_>,
        gather: u32,
        ids: &[QualifiedId],
        listener: Option<Listener>,
    ) -> Batch {
        if ids.is_empty() {
            return Batch::NONE;
        }
        let n = i32::try_from(ids.len()).unwrap_or(i32::MAX);
        let h = self.allocate_callback(listener, n);
        let batch = h.map_or(Batch::NONE, |h| self.hnd(h).batch);
        for q in ids {
            let i = self.form_get_request(gather, *q);
            if let Some(h) = h {
                self.add_top_level_request(h, i);
            }
            if self.req(i).all_done() {
                self.notify_callback(src, i);
            } else if self.req(i).flags & HASHED == 0 {
                self.hash_and_enqueue(src, i);
            }
            self.release(i);
        }
        if let Some(h) = h {
            self.release_h(h);
        }
        batch
    }

    /// Remove a record from its data file without waiting.
    // D2-12812
    pub fn async_purge(
        &mut self,
        src: Sources<'_>,
        q: QualifiedId,
        listener: Option<Listener>,
    ) -> Batch {
        let i = self.add_req(Request::new(Op::Purge, q));
        self.release_obj(src, i);
        let mut batch = Batch::NONE;
        if let Some(h) = self.allocate_callback(listener, 1) {
            self.add_top_level_request(h, i);
            batch = self.hnd(h).batch;
            self.release_h(h);
        }
        self.enqueue(src, i);
        self.release(i);
        batch
    }

    /// Remove several records from their data files in one batch.
    // D2-12813
    pub fn async_purge_many(
        &mut self,
        src: Sources<'_>,
        ids: &[QualifiedId],
        listener: Option<Listener>,
    ) -> Batch {
        let n = i32::try_from(ids.len()).unwrap_or(i32::MAX);
        let h = self.allocate_callback(listener, n);
        let batch = h.map_or(Batch::NONE, |h| self.hnd(h).batch);
        for q in ids {
            let i = self.add_req(Request::new(Op::Purge, *q));
            if let Some(h) = h {
                self.add_top_level_request(h, i);
            }
            self.enqueue(src, i);
            self.release(i);
        }
        if let Some(h) = h {
            self.release_h(h);
        }
        batch
    }

    /// Store a record's bytes in a data file (`dat_file`, or the one its type belongs to) without
    /// waiting.
    // D2-12814
    pub fn async_save(
        &mut self,
        src: Sources<'_>,
        q: QualifiedId,
        data: Vec<u8>,
        dat_file: u64,
        listener: Option<Listener>,
    ) -> Batch {
        let mut r = Request::new(Op::Save, q);
        r.buf = Some(data);
        r.dat_file = dat_file;
        let i = self.add_req(r);
        let mut batch = Batch::NONE;
        if let Some(h) = self.allocate_callback(listener, 1) {
            self.add_top_level_request(h, i);
            batch = self.hnd(h).batch;
            self.release_h(h);
        }
        self.enqueue(src, i);
        self.release(i);
        batch
    }

    // ------------------------------------------------------------------ finishing

    /// A request done: by what it did.
    // D2-12762
    fn on_request_finished(&mut self, src: Sources<'_>, i: usize) {
        match self.req(i).op {
            Op::Get => self.on_get_request_finished(src, i),
            Op::Purge => self.on_purge_request_finished(src, i),
            Op::Save => self.on_save_request_finished(src, i),
        }
    }

    /// A purge done: its batches are told.
    // D2-12763
    fn on_purge_request_finished(&mut self, src: Sources<'_>, i: usize) {
        self.notify_callback(src, i);
    }

    /// A save done: a get kept for the same record, not under way and not finished, takes its
    /// outcome and (stored) its bytes, and finishes with them; then the save's batches are told.
    fn on_save_request_finished(&mut self, src: Sources<'_>, i: usize) {
        let (q, outcome) = {
            let r = self.req(i);
            (r.q, r.outcome)
        };
        let data = self.req_mut(i).buf.take();
        self.hand_saved_to_get(src, q, outcome, data);
        self.notify_callback(src, i);
    }

    // D2-12800
    fn hand_saved_to_get(
        &mut self,
        src: Sources<'_>,
        q: QualifiedId,
        outcome: Outcome,
        data: Option<Vec<u8>>,
    ) {
        let Some(&g) = self.pending_gets.get(&q) else {
            return;
        };
        let r = self.req(g);
        if r.outcome != Outcome::Unknown || r.flags & WORKING != 0 {
            return;
        }
        self.req_mut(g).outcome = outcome;
        if outcome == Outcome::Success {
            let data = match data.filter(|d| !d.is_empty()) {
                Some(d) => Some(d),
                None => src.files.load_data(q),
            };
            self.req_mut(g).buf = data;
        }
        self.on_get_request_finished(src, g);
    }

    /// A record the server sent was stored (`data`: its bytes as stored): a get waiting for it
    /// takes them. (The data download stores what the server sends; this is the cache's part of
    /// that save finishing.)
    pub fn on_saved(&mut self, src: Sources<'_>, q: QualifiedId, data: Option<Vec<u8>>) {
        self.hand_saved_to_get(src, q, Outcome::Success, data);
    }

    /// The server could not send a record: a get kept for it fails.
    // D2-12805
    pub fn on_refused(&mut self, src: Sources<'_>, q: QualifiedId) {
        if let Some(&g) = self.pending_gets.get(&q) {
            self.req_mut(g).outcome = Outcome::Failure;
            self.on_get_request_finished(src, g);
        }
    }

    // ------------------------------------------------------------------ the worker

    /// The worker's part of a request: a get reads its record's bytes, a save stores them, a
    /// purge removes the record. Each fails unless it did.
    // D2-12817
    fn worker_execute_request(&mut self, src: Sources<'_>, i: usize) {
        match self.req(i).op {
            Op::Get => self.worker_execute_get_request(src, i),
            Op::Save => {
                let r = self.req(i);
                let stored = src.files.worker_execute_save_request(
                    r.q,
                    r.dat_file,
                    r.buf.as_deref().unwrap_or_default(),
                );
                if stored {
                    self.req_mut(i).outcome = Outcome::Success;
                }
            }
            Op::Purge => {
                let gone = src.files.worker_execute_purge_request(self.req(i).q);
                self.req_mut(i).outcome = if gone {
                    Outcome::Success
                } else {
                    Outcome::Failure
                };
            }
        }
    }

    // D2-12821
    fn worker_execute_get_request(&mut self, src: Sources<'_>, i: usize) {
        let q = self.req(i).q;
        let bytes = src.files.load_data(q);
        let r = self.req_mut(i);
        r.outcome = if bytes.is_some() {
            Outcome::Success
        } else {
            Outcome::Failure
        };
        r.buf = bytes;
    }

    /// The worker's turn: every queued request carried out, in order, each failing unless it
    /// did; none once the worker has stopped.
    // D2-12827
    pub fn run_worker(&mut self, src: Sources<'_>) {
        if self.stopped {
            return;
        }
        while let Some(i) = self.jobs.pop_front() {
            self.req_mut(i).outcome = Outcome::Failure;
            self.worker_execute_request(src, i);
            self.replies.push_back(i);
        }
    }

    /// Stop the worker: what is queued stays queued.
    // D2-12819, D2-12820
    pub fn stop_worker(&mut self) {
        self.stopped = true;
    }

    /// Shutting down (or not): the worker stops, and the records' caches keep no free records
    /// while shutting down. Whether the caches were left keeping them.
    // D2-12818
    pub fn set_shutdown(&mut self, src: Sources<'_>, shutting: bool) -> bool {
        self.stop_worker();
        !src.db.keep_free_objects(None, !shutting)
    }

    /// Give the cache time, `now` on the host's clock in seconds: the worker runs its queue, each
    /// request it did is finished in order, each batch done is told, and the records' caches
    /// are given the time.
    // D2-12824
    pub fn use_time(&mut self, src: Sources<'_>, now: f64) {
        self.run_worker(src);
        while let Some(i) = self.replies.pop_front() {
            self.on_request_finished(src, i);
            self.release(i);
        }
        self.tell_done_batches();
        src.db.use_time(now);
    }

    /// Let every batch go, and forget every get kept.
    // D2-12795, D2-12598
    pub fn flush_pending_requests(&mut self) {
        let batches: Vec<Batch> = self.busy.keys().copied().collect();
        for b in batches {
            self.release_batch(b);
        }
        let kept: Vec<usize> = std::mem::take(&mut self.pending_gets)
            .into_values()
            .collect();
        for i in kept {
            self.release(i);
        }
    }

    // ------------------------------------------------------------------ waiting gets

    /// A record now, for the gathering `gather`: from memory; else the record a kept get
    /// decoded, which goes into its cache; else read from the data files and decoded. A record got
    /// here counts its named records as loaded (unless gathered for none).
    // D2-12804
    pub fn blocking_get(
        &mut self,
        src: Sources<'_>,
        gather: u32,
        q: QualifiedId,
    ) -> Option<AnyObj> {
        if q.id == 0 {
            return None;
        }
        if !Record::has_decoder(RecordType(q.kind)) {
            return None;
        }
        let o = match in_memory(src, q, true) {
            Some(o) => o,
            None => match self.pending_gets.get(&q).copied() {
                Some(g) if self.req(g).obj.is_some() => {
                    let r = self.req_mut(g);
                    r.flags |= IN_CACHE;
                    let obj = r.obj.take()?;
                    let held = match obj {
                        Loaded::InCache(o) => o,
                        Loaded::Fresh { kind, key, value } => {
                            add_obj_to_cache(src, kind, key, value)
                                .or_else(|| cache::in_memory_any(src.db.caches(), kind, key))?
                        }
                    };
                    let again = held.clone();
                    self.req_mut(g).obj = Some(Loaded::InCache(held));
                    again
                }
                _ => blocking_get_from_disk(src, q)?,
            },
        };
        if gather != GATHER_NONE {
            cache::set_subs_loaded(src.db.caches(), q.kind, (q.id, rust_type(q.kind)));
        }
        Some(o)
    }
}

/// A batch's outcome so far with one more request's: a success after a failure, or anything
/// but a success after a success, is a partial success; the first outcome is taken as it is.
fn sum(so_far: Outcome, one: Outcome) -> Outcome {
    match (so_far, one) {
        (Outcome::Failure, Outcome::Success) => Outcome::Partial,
        (Outcome::Unknown, o) => o,
        (Outcome::Success, o) if o != Outcome::Success => Outcome::Partial,
        (s, _) => s,
    }
}

/// The Rust type a record type is held by (one no decoder knows: `()`, which nothing is).
fn rust_type(kind: u32) -> std::any::TypeId {
    Record::rust_type(RecordType(kind)).unwrap_or(std::any::TypeId::of::<()>())
}

/// A loaded record from its type's cache, when its type has one.
// D2-12765
fn in_memory(src: Sources<'_>, q: QualifiedId, has_cache: bool) -> Option<AnyObj> {
    if !has_cache {
        return None;
    }
    cache::in_memory_any(src.db.caches(), q.kind, (q.id, rust_type(q.kind)))
}

/// A record decoded from its bytes by its type, held alone.
fn decode(src: Sources<'_>, q: QualifiedId, bytes: &[u8]) -> Option<Held> {
    Record::decode(RecordType(q.kind), q.id, bytes, src.db.context())?
        .ok()
        .map(Record::into_held)
}

/// The records record `q` names, for a gathering.
fn sub_ids(
    src: Sources<'_>,
    q: QualifiedId,
    value: &(dyn std::any::Any + Send + Sync),
    gather: u32,
) -> Vec<QualifiedId> {
    let mut out = QualifiedIds::new();
    record_sub_ids(src.states, q.id, value, gather, &mut out);
    out.iter().collect()
}

/// The records record `id` names for gathering `gather`, added to `out`: a saved script state's
/// as `states` answers (none without it), any other's as its type gathers them.
fn record_sub_ids(
    states: Option<&dyn StateRecords>,
    id: u32,
    value: &dyn std::any::Any,
    gather: u32,
    out: &mut QualifiedIds,
) {
    match (states, value.downcast_ref::<ScriptState>()) {
        (Some(s), Some(state)) => s.state_records(id, state, gather, out),
        _ => gather_sub_ids(value, gather, out),
    }
}

/// Put a record decoded alone into its type's cache, held once more (into a data graph when one
/// is installed); none when its id is loaded already.
// D2-12764
fn add_obj_to_cache(src: Sources<'_>, kind: u32, key: Key, value: Held) -> Option<AnyObj> {
    let graphed: Arc<dyn std::any::Any + Send + Sync> = value.clone();
    let o = cache::add_any(src.db.caches(), kind, key, value)?;
    src.db.add_to_data_graph(key.0, kind, &*graphed);
    Some(o)
}

/// Read a record from its data file and decode it into its type's cache.
// D2-12773
fn blocking_get_from_disk(src: Sources<'_>, q: QualifiedId) -> Option<AnyObj> {
    if q.id == 0 {
        return None;
    }
    cache::get_free_obj(src.db.caches(), q.kind);
    let value = blocking_load_into(src, q)?;
    add_obj_to_cache(src, q.kind, (q.id, rust_type(q.kind)), value)
}

/// A record read from its data file and decoded.
// D2-12781
fn blocking_load_into(src: Sources<'_>, q: QualifiedId) -> Option<Held> {
    if q.id == 0 {
        return None;
    }
    let bytes = src.files.load_data(q)?;
    decode(src, q, &bytes)
}
