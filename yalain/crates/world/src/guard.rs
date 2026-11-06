//! A panic in the world stops the world.
//!
//! The world's work changes many records by hand with nothing to take it back: a character can be
//! out of [`Server::players`] while a step runs, an item can have left one holder and not reached
//! another. So after a panic the memory can't be told from the truth, and keeping one player out of
//! the saves wouldn't do: what it had reached another player would be held twice once the other is
//! kept. So, whatever panicked and however small it looks:
//!
//! - what whole pieces of work held goes to the store, all a pass had kept before it, and all that
//!   saves failing had been holding, the way a stop writes it;
//! - what the piece that panicked held is put back as it was, and no one's memory is saved again:
//!   each character goes back to its last save, whole, and every item that moved was saved with both
//!   its sides, so the store has each in one place;
//! - everyone in the world is told, and every connection is ended;
//! - the operator reads an error line, and the server exits with a failure, so a service manager
//!   starts it again and no staged update goes in.
//!
//! A world that runs no pass for a long time ends the same way, by the [`PassWatch`] outside its
//! thread: the failure's exit, each character back to its last save. What the world holds can't be
//! reached from outside its thread, so nothing is written or told first.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use auberean_protocol::login::CharacterRefused;
use yalain_net::ServerNet;
use yalain_store::Store;

use crate::{Clock, Server};

/// What each player in the world is told as it stops.
pub const STOPPING: &str =
    "This world hit an error and is stopping. You'll be back as of your last save.";

/// What a panic carried, as words.
#[must_use]
pub fn words(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no words".to_owned())
}

/// Something in the world's work panicked: from here on nothing of its memory is saved, what the
/// piece under way held is put back, and everyone is told and let go. The caller then writes out what
/// is held, as any stop does.
pub fn stopped<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock, what: &str) {
    s.saves.take_back();
    s.panicked = Some(what.to_owned());
    tracing::error!(
        "THE WORLD PANICKED: {what}. It is stopping: what whole events kept is written, and nothing \
         since. Characters go back to their last saves. This is a bug; the line above says where."
    );
    // The connections may be what panicked; the store's writing must still come after.
    let told = catch_unwind(AssertUnwindSafe(|| {
        for world in s.players.keys() {
            crate::login::notice(net, *world, STOPPING);
        }
        crate::login::let_everyone_go(s, net, CharacterRefused::SERVER_CRASH, clock.local);
    }));
    if told.is_err() {
        tracing::error!("the connections could not be told the world is stopping");
    }
}

/// How long a world may run no pass before it is stopped, in seconds, unless its operator says
/// otherwise. Far above the longest pass known, so that only a world not running trips it: 145 ms,
/// a world's first pass on the whole world, and a save on a locked store at most a second a character.
pub const NO_PASS_SECONDS: u32 = 300;

/// Where in its pass the world is: the part of the pass last begun.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Part {
    /// The operator's console and what its jobs finished.
    Console,
    /// What came over the network.
    Network,
    /// The world's events and timers.
    World,
    /// The services beside the world and what goes out.
    Sending,
}

impl Part {
    const ALL: [Self; 4] = [Self::Console, Self::Network, Self::World, Self::Sending];

    /// What the operator's log calls it.
    #[must_use]
    pub const fn words(self) -> &'static str {
        match self {
            Self::Console => "the console",
            Self::Network => "what came over the network",
            Self::World => "the world's events and timers",
            Self::Sending => "the services and what goes out",
        }
    }
}

/// The world's passes as its thread counts them, for the watch on another thread to read.
#[derive(Debug, Clone, Default)]
pub struct Passes {
    count: Arc<AtomicU64>,
    part: Arc<AtomicU8>,
    done: Arc<AtomicBool>,
}

impl Passes {
    /// One more pass is under way.
    pub fn pass(&self) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.at(Part::Console);
    }

    /// The pass has come to a part.
    pub fn at(&self, part: Part) {
        self.part.store(part as u8, Ordering::Relaxed);
    }

    /// The world's passes are over: it is stopping as it should, and the watch ends.
    pub fn over(&self) {
        self.done.store(true, Ordering::Relaxed);
    }

    /// The part the last pass reached.
    #[must_use]
    pub fn part(&self) -> Part {
        Part::ALL
            .get(usize::from(self.part.load(Ordering::Relaxed)))
            .copied()
            .unwrap_or(Part::Console)
    }
}

/// A look at the world's passes every so often: `limit` looks in a row that see no new pass trip it.
/// It counts its own looks and reads no clock, so that a machine put to sleep and woken up isn't
/// taken for a world not running.
#[derive(Debug)]
pub struct PassWatch {
    limit: u32,
    last: Option<u64>,
    still: u32,
}

impl PassWatch {
    /// A watch that `limit` looks trip; 0 never trips.
    #[must_use]
    pub const fn new(limit: u32) -> Self {
        Self {
            limit,
            last: None,
            still: 0,
        }
    }

    /// Look at the count of passes: has the world run none for the limit?
    pub fn look(&mut self, count: u64) -> bool {
        if self.last == Some(count) {
            self.still = self.still.saturating_add(1);
        } else {
            self.last = Some(count);
            self.still = 0;
        }
        self.limit != 0 && self.still >= self.limit
    }
}

/// Watch the world's passes, `rest` between looks, until they are over; or call `stop` and end, once,
/// when the world has run none for `limit` looks. Returns whether it called `stop`.
pub fn watch(
    passes: &Passes,
    limit: u32,
    mut rest: impl FnMut(),
    mut stop: impl FnMut(Part),
) -> bool {
    let mut w = PassWatch::new(limit);
    loop {
        if passes.done.load(Ordering::Relaxed) {
            return false;
        }
        if w.look(passes.count.load(Ordering::Relaxed)) {
            stop(passes.part());
            return true;
        }
        rest();
    }
}
