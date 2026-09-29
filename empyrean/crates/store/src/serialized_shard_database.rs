// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SerializedShardDatabase.cs
//! `SerializedShardDatabase`: the database thread. The world hands it owned snapshots; the jobs run
//! in order on one thread; each job's callback comes back to the world, which runs it on its own
//! thread when it drains [`ShardHandle::take_completed`].
//!
//! ACE runs each queued `Task` and invokes its callback on the database thread. Here:
//! * callbacks are `Box<dyn FnOnce(&mut W, R) + Send>` and run on the world thread (`W` is the
//!   world type; empyrean-store does not depend on empyrean-world). DIVERGE (arch, the point of the design).
//! * The batched writer (carried from the v1 server): the thread takes every queued job, up to
//!   [`MERGE_LIMIT`], and runs them in one transaction; callbacks are released, in order, only after
//!   the commit, so a reported success is durable. ACE commits each save on its own.
//! * A failed commit stops the writer: the batch is rolled back, its write callbacks get `false`, and
//!   every later write is refused with `false` without running. Reads still run. ACE logs the
//!   exception and carries on (`DoWork` swallows it), which can lose writes silently.
//! * A job that panics (ACE: throws) is logged and its callback is dropped, as in ACE's `DoWork`.
//! * The queue is unbounded, as ACE's `BlockingCollection` is.
//! * [`ShardHandle::synchronous`] runs each job inline, in its own batch, and queues the callback:
//!   tests stay deterministic and use no threads.
//!
//! [`ShardHandle::base_database`] is ACE's `BaseDatabase` for the synchronous calls the world makes
//! directly (landblock loads, houses, allegiances). It takes the same lock as the database thread,
//! so it waits for at most the batch in progress.

use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use empyrean_common::clock::Clock;
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_entity::enums::AccessLevel;

use crate::entity::PossessedBiotas;
use crate::models::shard::{Biota, Character};
use crate::shard_database::ShardDatabase;

/// The most jobs one transaction takes (v1's value).
pub const MERGE_LIMIT: usize = 64;

/// A callback the world runs, with the job's result.
pub type Callback<W, R> = Box<dyn FnOnce(&mut W, R) + Send>;

/// A finished job's world-side half.
pub type Completed<W> = Box<dyn FnOnce(&mut W) + Send>;

/// The shared database a [`ShardHandle`] and its thread use.
pub type SharedShard = Arc<Mutex<Box<dyn ShardDatabase>>>;

/// What a job left for the world.
enum Outcome<W> {
    /// A read: deliver whatever happens to the batch.
    Read(Option<Completed<W>>),
    /// A write: `ok` after a commit, `failed` (the callback with `false`) when the commit fails.
    Write {
        ok: Option<Completed<W>>,
        failed: Option<Completed<W>>,
    },
}

type JobFn<W> = Box<dyn FnOnce(&mut dyn ShardDatabase) -> Outcome<W> + Send>;

struct Job<W> {
    is_write: bool,
    run: JobFn<W>,
    /// The callback with `false`, for a write refused by a stopped writer.
    refuse: Option<Completed<W>>,
}

struct Shared {
    queue_count: AtomicUsize,
    dead: AtomicBool,
    in_transaction: AtomicBool,
}

/// Watches the database thread from any thread (see [`ShardHandle::probe`]).
#[derive(Clone)]
pub struct WriterProbe(Arc<Shared>);

impl WriterProbe {
    /// Whether the database thread is inside a batch transaction.
    #[must_use]
    pub fn in_transaction(&self) -> bool {
        self.0.in_transaction.load(Ordering::SeqCst)
    }

    /// Whether a failed commit has stopped the writer.
    #[must_use]
    pub fn is_writer_stopped(&self) -> bool {
        self.0.dead.load(Ordering::SeqCst)
    }
}

impl std::fmt::Debug for WriterProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriterProbe")
            .field("in_transaction", &self.in_transaction())
            .finish()
    }
}

enum Mode<W> {
    Threaded {
        tx: Option<Sender<Job<W>>>,
        worker: Option<JoinHandle<()>>,
    },
    Synchronous,
}

// ACE: SerializedShardDatabase
/// The world's handle on the database thread.
pub struct ShardHandle<W: 'static> {
    db: SharedShard,
    clock: Arc<dyn Clock>,
    shared: Arc<Shared>,
    mode: Mode<W>,
    done_tx: Sender<Completed<W>>,
    done_rx: Receiver<Completed<W>>,
}

impl<W: 'static> std::fmt::Debug for ShardHandle<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardHandle")
            .field("threaded", &matches!(self.mode, Mode::Threaded { .. }))
            .field(
                "queue_count",
                &self.shared.queue_count.load(Ordering::Relaxed),
            )
            .field("dead", &self.shared.dead.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

fn lock(db: &SharedShard) -> MutexGuard<'_, Box<dyn ShardDatabase>> {
    db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Runs one batch: begin, every job (a panicking job is logged and dropped), commit. Returns the
/// completions in job order.
fn run_batch<W>(db: &SharedShard, shared: &Shared, jobs: Vec<Job<W>>) -> Vec<Completed<W>> {
    let mut guard = lock(db);
    let mut out: Vec<Outcome<W>> = Vec::with_capacity(jobs.len());
    let dead = shared.dead.load(Ordering::SeqCst);

    let mut refused: Vec<(usize, Completed<W>)> = Vec::new();
    let began = match guard.begin_batch() {
        Ok(()) => true,
        Err(e) => {
            log::error!("[DATABASE] could not begin a transaction: {e}");
            false
        }
    };
    shared.in_transaction.store(began, Ordering::SeqCst);

    for (i, job) in jobs.into_iter().enumerate() {
        shared.queue_count.fetch_sub(1, Ordering::SeqCst);
        if job.is_write && (dead || !began) {
            if let Some(r) = job.refuse {
                refused.push((i, r));
            }
            out.push(Outcome::Read(None));
            continue;
        }
        let db_ref: &mut dyn ShardDatabase = guard.as_mut();
        match catch_unwind(AssertUnwindSafe(|| (job.run)(db_ref))) {
            Ok(outcome) => out.push(outcome),
            Err(panic) => {
                let msg = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("(non-string panic)");
                // perhaps add failure callbacks?
                // swallow for now.  can't block other db work because 1 fails.
                log::error!("[DATABASE] DoWork task failed with exception: {msg}");
                out.push(Outcome::Read(None));
            }
        }
    }

    let committed = if began {
        match guard.commit_batch() {
            Ok(()) => true,
            Err(e) => {
                log::error!("[DATABASE] commit failed, the database writer is stopped: {e}");
                shared.dead.store(true, Ordering::SeqCst);
                false
            }
        }
    } else {
        false
    };
    shared.in_transaction.store(false, Ordering::SeqCst);
    drop(guard);

    let mut completions = Vec::with_capacity(out.len());
    let mut refused = refused.into_iter().peekable();
    for (i, outcome) in out.into_iter().enumerate() {
        if refused.peek().is_some_and(|(j, _)| *j == i) {
            completions.push(refused.next().unwrap_or_else(|| unreachable!()).1);
            continue;
        }
        match outcome {
            Outcome::Read(c) => completions.extend(c),
            Outcome::Write { ok, failed } => {
                completions.extend(if committed { ok } else { failed })
            }
        }
    }
    completions
}

fn worker_loop<W>(
    db: SharedShard,
    shared: Arc<Shared>,
    rx: Receiver<Job<W>>,
    done_tx: Sender<Completed<W>>,
) {
    // ACE: SerializedShardDatabase.DoWork
    let mut pending: VecDeque<Job<W>> = VecDeque::new();
    loop {
        if pending.is_empty() {
            match rx.recv() {
                Ok(job) => pending.push_back(job),
                Err(_) => break, // _queue is empty and CompleteForAdding has been called -- we're done here
            }
        }
        while pending.len() < MERGE_LIMIT {
            match rx.try_recv() {
                Ok(job) => pending.push_back(job),
                Err(_) => break,
            }
        }
        let take = pending.len().min(MERGE_LIMIT);
        let batch: Vec<Job<W>> = pending.drain(..take).collect();
        for c in run_batch(&db, &shared, batch) {
            // The world may already be gone at shutdown; nothing to deliver to then.
            let _ = done_tx.send(c);
        }
    }
}

impl<W: 'static> ShardHandle<W> {
    fn with_mode(db: Box<dyn ShardDatabase>, clock: Arc<dyn Clock>, threaded: bool) -> Self {
        let (done_tx, done_rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            queue_count: AtomicUsize::new(0),
            dead: AtomicBool::new(false),
            in_transaction: AtomicBool::new(false),
        });
        let db: SharedShard = Arc::new(Mutex::new(db));
        let mode = if threaded {
            Mode::Threaded {
                tx: None,
                worker: None,
            }
        } else {
            Mode::Synchronous
        };
        Self {
            db,
            clock,
            shared,
            mode,
            done_tx,
            done_rx,
        }
    }

    // ACE: SerializedShardDatabase.SerializedShardDatabase
    /// A handle whose jobs run on a database thread (call [`ShardHandle::start`]).
    #[must_use]
    pub fn new(shard_database: Box<dyn ShardDatabase>, clock: Arc<dyn Clock>) -> Self {
        Self::with_mode(shard_database, clock, true)
    }

    /// A handle that runs every job inline, in its own transaction, and queues its callback.
    #[must_use]
    pub fn synchronous(shard_database: Box<dyn ShardDatabase>, clock: Arc<dyn Clock>) -> Self {
        Self::with_mode(shard_database, clock, false)
    }

    // ACE: SerializedShardDatabase.Start
    /// Starts the database thread ("Serialized Shard Database"). No effect in synchronous mode or
    /// when already started.
    ///
    /// # Panics
    /// If the thread cannot be spawned.
    pub fn start(&mut self) {
        if let Mode::Threaded { tx, worker } = &mut self.mode {
            if worker.is_some() {
                return;
            }
            let (job_tx, job_rx) = mpsc::channel::<Job<W>>();
            let db = Arc::clone(&self.db);
            let shared = Arc::clone(&self.shared);
            let done_tx = self.done_tx.clone();
            let handle = std::thread::Builder::new()
                .name("Serialized Shard Database".into())
                .spawn(move || worker_loop(db, shared, job_rx, done_tx))
                .expect("spawn the database thread");
            *tx = Some(job_tx);
            *worker = Some(handle);
        }
    }

    // ACE: SerializedShardDatabase.Stop
    /// Finishes the queued jobs and stops the thread.
    pub fn stop(&mut self) {
        if let Mode::Threaded { tx, worker } = &mut self.mode {
            tx.take(); // _queue.CompleteAdding();
            if let Some(w) = worker.take() {
                let _ = w.join();
            }
        }
    }

    /// The callbacks of the jobs that finished since the last call, in order. The world runs each
    /// with itself: `for c in handle.take_completed() { c(world) }`.
    pub fn take_completed(&self) -> Vec<Completed<W>> {
        self.done_rx.try_iter().collect()
    }

    /// Runs every finished callback on `w`, in order (when the handle is not part of `W`).
    pub fn drain(&self, w: &mut W) {
        for c in self.take_completed() {
            c(w);
        }
    }

    /// Whether a failed commit has stopped the writer.
    #[must_use]
    pub fn is_writer_stopped(&self) -> bool {
        self.shared.dead.load(Ordering::SeqCst)
    }

    /// Whether the database thread is inside a batch transaction (for the durability gate).
    #[must_use]
    pub fn in_transaction(&self) -> bool {
        self.shared.in_transaction.load(Ordering::SeqCst)
    }

    /// A handle another thread can hold to watch the writer (for the durability gate).
    #[must_use]
    pub fn probe(&self) -> WriterProbe {
        WriterProbe(Arc::clone(&self.shared))
    }

    // ACE: SerializedShardDatabase.BaseDatabase
    /// ACE's `BaseDatabase`: the database for a direct, synchronous call.
    pub fn base_database(&self) -> MutexGuard<'_, Box<dyn ShardDatabase>> {
        lock(&self.db)
    }

    fn enqueue(&self, job: Job<W>) {
        self.shared.queue_count.fetch_add(1, Ordering::SeqCst);
        match &self.mode {
            Mode::Synchronous => {
                for c in run_batch(&self.db, &self.shared, vec![job]) {
                    let _ = self.done_tx.send(c);
                }
            }
            Mode::Threaded { tx: Some(tx), .. } => {
                if let Err(mpsc::SendError(job)) = tx.send(job) {
                    self.shared.queue_count.fetch_sub(1, Ordering::SeqCst);
                    log::error!("[DATABASE] the database thread is gone; a job was dropped");
                    if let Some(r) = job.refuse {
                        let _ = self.done_tx.send(r);
                    }
                }
            }
            Mode::Threaded { tx: None, .. } => {
                // ACE's BlockingCollection accepts tasks before Start; they run once it starts.
                // A handle that was never started (or was stopped) runs nothing.
                self.shared.queue_count.fetch_sub(1, Ordering::SeqCst);
                log::error!(
                    "[DATABASE] job submitted while the database thread is not running; dropped"
                );
                if let Some(r) = job.refuse {
                    let _ = self.done_tx.send(r);
                }
            }
        }
    }

    fn read<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut dyn ShardDatabase) -> R + Send + 'static,
        callback: Option<Callback<W, R>>,
    ) {
        let run: JobFn<W> = Box::new(move |db| {
            let result = f(db);
            Outcome::Read(
                callback.map(|cb| -> Completed<W> { Box::new(move |w: &mut W| cb(w, result)) }),
            )
        });
        self.enqueue(Job {
            is_write: false,
            run,
            refuse: None,
        });
    }

    fn write(
        &self,
        f: impl FnOnce(&mut dyn ShardDatabase) -> bool + Send + 'static,
        callback: Option<Callback<W, bool>>,
    ) {
        let shared_cb = Arc::new(Mutex::new(callback));
        let refuse_cb = Arc::clone(&shared_cb);
        let refuse: Completed<W> = Box::new(move |w: &mut W| {
            if let Some(cb) = refuse_cb
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            {
                cb(w, false);
            }
        });
        let run: JobFn<W> = Box::new(move |db| {
            let result = f(db);
            let ok_cb = Arc::clone(&shared_cb);
            let failed_cb = shared_cb;
            Outcome::Write {
                ok: Some(Box::new(move |w: &mut W| {
                    if let Some(cb) = ok_cb
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take()
                    {
                        cb(w, result);
                    }
                })),
                failed: Some(Box::new(move |w: &mut W| {
                    if let Some(cb) = failed_cb
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take()
                    {
                        cb(w, false);
                    }
                })),
            }
        });
        self.enqueue(Job {
            is_write: true,
            run,
            refuse: Some(refuse),
        });
    }

    // ACE: SerializedShardDatabase.QueueCount
    /// Jobs queued and not yet started.
    #[must_use]
    pub fn queue_count(&self) -> i32 {
        i32::try_from(self.shared.queue_count.load(Ordering::SeqCst)).unwrap_or(i32::MAX)
    }

    // ACE: SerializedShardDatabase.GetCurrentQueueWaitTime
    /// Measures how long a job waits in the queue.
    pub fn get_current_queue_wait_time(&self, callback: Option<Callback<W, TimeSpan>>) {
        let clock = Arc::clone(&self.clock);
        let initial_call_time = clock.utc_now();
        self.read(move |_| clock.utc_now() - initial_call_time, callback);
    }

    // ACE: SerializedShardDatabase.GetMaxGuidFoundInRange
    /// Will return `u32::MAX` if no records were found within the range provided.
    pub fn get_max_guid_found_in_range(
        &self,
        min: u32,
        max: u32,
        callback: Option<Callback<W, u32>>,
    ) {
        self.read(move |db| db.get_max_guid_found_in_range(min, max), callback);
    }

    // ACE: SerializedShardDatabase.GetSequenceGaps
    /// Available ids, as gaps in the id sequence after `min`.
    pub fn get_sequence_gaps(
        &self,
        min: u32,
        limit_available_ids_returned: u32,
        callback: Option<Callback<W, Vec<(u32, u32)>>>,
    ) {
        self.read(
            move |db| db.get_sequence_gaps(min, limit_available_ids_returned),
            callback,
        );
    }

    // ACE: SerializedShardDatabase.SaveBiota
    /// Saves a snapshot of an entity biota.
    pub fn save_biota(
        &self,
        mut biota: empyrean_entity::Biota,
        callback: Option<Callback<W, bool>>,
    ) {
        self.write(move |db| db.save_biota(&mut biota, false), callback);
    }

    // ACE: SerializedShardDatabase.SaveBiotasInParallel
    /// Saves snapshots of several entity biotas.
    pub fn save_biotas_in_parallel(
        &self,
        mut biotas: Vec<empyrean_entity::Biota>,
        callback: Option<Callback<W, bool>>,
        do_not_add_to_cache: bool,
    ) {
        self.write(
            move |db| db.save_biotas_in_parallel(&mut biotas, do_not_add_to_cache),
            callback,
        );
    }

    // ACE: SerializedShardDatabase.RemoveBiota
    pub fn remove_biota(&self, id: u32, callback: Option<Callback<W, bool>>) {
        self.write(move |db| db.remove_biota(id), callback);
    }

    // ACE: SerializedShardDatabase.RemoveBiota
    /// `RemoveBiota` with ACE's performance report: the time queued and the time taken.
    pub fn remove_biota_timed(
        &self,
        id: u32,
        callback: Option<Callback<W, bool>>,
        performance_results: Option<Callback<W, (TimeSpan, TimeSpan)>>,
    ) {
        let clock = Arc::clone(&self.clock);
        let initial_call_time = clock.utc_now();
        let times = Arc::new(Mutex::new((TimeSpan::ZERO, TimeSpan::ZERO)));
        let times_w = Arc::clone(&times);
        let callback: Option<Callback<W, bool>> = Some(Box::new(move |w: &mut W, result: bool| {
            if let Some(cb) = callback {
                cb(w, result);
            }
            let t = *times
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(p) = performance_results {
                p(w, t);
            }
        }));
        self.write(
            move |db| {
                let task_start_time = clock.utc_now();
                let result = db.remove_biota(id);
                let task_completed_time = clock.utc_now();
                *times_w
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = (
                    task_start_time - initial_call_time,
                    task_completed_time - task_start_time,
                );
                result
            },
            callback,
        );
    }

    // ACE: SerializedShardDatabase.RemoveBiotasInParallel
    pub fn remove_biotas_in_parallel(
        &self,
        ids: Vec<u32>,
        callback: Option<Callback<W, bool>>,
        performance_results: Option<Callback<W, (TimeSpan, TimeSpan)>>,
    ) {
        let clock = Arc::clone(&self.clock);
        let initial_call_time = clock.utc_now();
        let times = Arc::new(Mutex::new((TimeSpan::ZERO, TimeSpan::ZERO)));
        let times_w = Arc::clone(&times);
        let callback: Option<Callback<W, bool>> = Some(Box::new(move |w: &mut W, result: bool| {
            if let Some(cb) = callback {
                cb(w, result);
            }
            let t = *times
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(p) = performance_results {
                p(w, t);
            }
        }));
        self.write(
            move |db| {
                let task_start_time = clock.utc_now();
                let result = db.remove_biotas_in_parallel(&ids);
                let task_completed_time = clock.utc_now();
                *times_w
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = (
                    task_start_time - initial_call_time,
                    task_completed_time - task_start_time,
                );
                result
            },
            callback,
        );
    }

    // ACE: SerializedShardDatabase.GetPossessedBiotasInParallel
    pub fn get_possessed_biotas_in_parallel(
        &self,
        id: u32,
        callback: Option<Callback<W, PossessedBiotas>>,
    ) {
        self.read(move |db| db.get_possessed_biotas_in_parallel(id), callback);
    }

    // ACE: SerializedShardDatabase.GetInventoryInParallel
    pub fn get_inventory_in_parallel(
        &self,
        parent_id: u32,
        included_nested_items: bool,
        callback: Option<Callback<W, Vec<Biota>>>,
    ) {
        self.read(
            move |db| db.get_inventory_in_parallel(parent_id, included_nested_items),
            callback,
        );
    }

    // ACE: SerializedShardDatabase.IsCharacterNameAvailable
    pub fn is_character_name_available(&self, name: String, callback: Option<Callback<W, bool>>) {
        self.read(move |db| db.is_character_name_available(&name), callback);
    }

    // ACE: SerializedShardDatabase.GetCharacters
    pub fn get_characters(
        &self,
        account_id: u32,
        include_deleted: bool,
        callback: Option<Callback<W, Vec<Character>>>,
    ) {
        self.read(
            move |db| db.get_characters(account_id, include_deleted),
            callback,
        );
    }

    // ACE: SerializedShardDatabase.GetCharacter
    pub fn get_character(
        &self,
        character_id: u32,
        callback: Option<Callback<W, Option<Character>>>,
    ) {
        self.read(move |db| db.get_character(character_id), callback);
    }

    // ACE: SerializedShardDatabase.SaveCharacter
    /// Saves a snapshot of a character.
    pub fn save_character(&self, character: Character, callback: Option<Callback<W, bool>>) {
        self.write(move |db| db.save_character(&character), callback);
    }

    // ACE: SerializedShardDatabase.RenameCharacter
    /// Renames and saves a snapshot of a character. The world's own copy is not renamed (ACE
    /// renames the shared object); the caller sets the name on its copy.
    pub fn rename_character(
        &self,
        mut character: Character,
        new_name: String,
        callback: Option<Callback<W, bool>>,
    ) {
        self.write(
            move |db| db.rename_character(&mut character, &new_name),
            callback,
        );
    }

    // ACE: SerializedShardDatabase.SetCharacterAccessLevelByName
    /// Not implemented in ACE either.
    ///
    /// # Panics
    /// Always: ACE throws `NotImplementedException` ("TODO").
    pub fn set_character_access_level_by_name(
        &self,
        _name: &str,
        _access_level: AccessLevel,
        _callback: Option<Callback<W, u32>>,
    ) {
        // TODO
        panic!("NotImplementedException: SerializedShardDatabase.SetCharacterAccessLevelByName");
    }

    // ACE: SerializedShardDatabase.AddCharacterInParallel
    /// Saves a new character: its biota, its possessions, then the character (all snapshots).
    pub fn add_character_in_parallel(
        &self,
        mut biota: empyrean_entity::Biota,
        mut possessions: Vec<empyrean_entity::Biota>,
        character: Character,
        callback: Option<Callback<W, bool>>,
    ) {
        self.write(
            move |db| db.add_character_in_parallel(&mut biota, &mut possessions, &character),
            callback,
        );
    }
}

impl<W: 'static> Drop for ShardHandle<W> {
    fn drop(&mut self) {
        self.stop();
    }
}
