//! The game clock, kept as the client keeps it: its timer over the host's elapsed time (read as
//! the timer reads it, never earlier than the last reading), the server's time moving the timer's
//! offset forward. Before the server's first time the game clock is the elapsed time; the
//! server's time never sets it back; and it never runs back.

use auberean_primitives::time::{Elapsed, Timer};

/// The game clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct Clock {
    timer: Timer,
    elapsed: Elapsed,
}

impl Clock {
    /// The clock brought up to the host's `elapsed` seconds, as at the start of each frame.
    pub fn update(&mut self, elapsed: f64) {
        let e = self.elapsed.answer(elapsed);
        self.timer.update(e);
    }

    /// The server's time, at the host's `elapsed` seconds: the clock moves forward to it when it
    /// is ahead, else stays as it is.
    pub fn set(&mut self, server: f64, elapsed: f64) {
        let e = self.elapsed.answer(elapsed);
        self.timer.set(server, e);
    }

    /// The game time now, in seconds.
    #[must_use]
    pub const fn now(&self) -> f64 {
        self.timer.current()
    }
}
