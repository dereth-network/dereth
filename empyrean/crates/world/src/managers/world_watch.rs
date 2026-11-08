//! Not ACE (V508): a watch on the world thread from outside it.
//!
//! The loop's stages catch a panic and go on, but nothing there can see the loop itself stuck. So
//! the loop counts its passes here, and a thread outside the world reads the count: a world that has
//! run no pass for a long while exits with a failure, so a service manager starts it again and no
//! staged update goes in. What the world holds can't be reached from another thread, so nothing is
//! written or told first, and each character goes back to its last save.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The stage named before any has started.
const NO_STAGE: &str = "the start of the loop";

/// The world's passes as its thread counts them, for the watch on another thread to read.
#[derive(Debug, Clone)]
pub struct Passes {
    count: Arc<AtomicU64>,
    stage: Arc<Mutex<&'static str>>,
    done: Arc<AtomicBool>,
}

impl Default for Passes {
    fn default() -> Self {
        Self {
            count: Arc::default(),
            stage: Arc::new(Mutex::new(NO_STAGE)),
            done: Arc::default(),
        }
    }
}

impl Passes {
    /// One more pass is under way.
    pub fn pass(&self) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// The pass has come to the stage `name`.
    pub fn at(&self, name: &'static str) {
        if let Ok(mut stage) = self.stage.lock() {
            *stage = name;
        }
    }

    /// The world's passes are over: it is stopping as it should, and the watch ends.
    pub fn over(&self) {
        self.done.store(true, Ordering::Relaxed);
    }

    /// How many passes have begun.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Whether the passes are over.
    #[must_use]
    pub fn is_over(&self) -> bool {
        self.done.load(Ordering::Relaxed)
    }

    /// The stage the last pass reached.
    #[must_use]
    pub fn stage(&self) -> &'static str {
        self.stage.lock().map_or(NO_STAGE, |stage| *stage)
    }
}

/// A look at the world's passes every so often: `limit` looks in a row that see no new pass trip it.
/// It counts its own looks and reads no clock, so a machine put to sleep and woken up isn't taken for
/// a world not running. The time before the first pass isn't counted: loading the preloaded
/// landblocks can take as long as it takes.
#[derive(Debug)]
pub struct PassWatch {
    limit: u32,
    last: u64,
    still: u32,
}

impl PassWatch {
    /// A watch that `limit` looks trip; 0 never trips.
    #[must_use]
    pub const fn new(limit: u32) -> Self {
        Self {
            limit,
            last: 0,
            still: 0,
        }
    }

    /// Look at the count of passes: has the world run none for the limit?
    pub fn look(&mut self, count: u64) -> bool {
        if count == 0 || count != self.last {
            self.last = count;
            self.still = 0;
            return false;
        }
        self.still = self.still.saturating_add(1);
        self.limit != 0 && self.still >= self.limit
    }
}

/// Watch the world's passes, `rest` between looks, until they are over; or call `stop` and end, once,
/// when the world has run none for `limit` looks. Returns whether it called `stop`.
pub fn watch(
    passes: &Passes,
    limit: u32,
    mut rest: impl FnMut(),
    mut stop: impl FnMut(&'static str),
) -> bool {
    let mut w = PassWatch::new(limit);
    loop {
        if passes.is_over() {
            return false;
        }
        if w.look(passes.count()) {
            stop(passes.stage());
            return true;
        }
        rest();
    }
}
