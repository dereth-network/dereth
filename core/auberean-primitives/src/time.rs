//! The client's clock, kept over the elapsed time the host gives it: the current time (the game's,
//! once the server has set it) and the local time (since the clock started), and the elapsed time
//! as each thread is answered it, which never goes back.
//!
//! Nothing here reads a clock: every call takes the host's elapsed seconds.

/// The client's clock. The current time is the host's elapsed time plus an offset the server's time
/// moves forward; it never goes back. The local time is the current time less the offset.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timer {
    base: f64,
    current: f64,
    local: f64,
}

impl Timer {
    /// A clock started: no offset, both times 0.
    // D2-17624
    #[must_use]
    pub const fn new() -> Self {
        Self {
            base: 0.0,
            current: 0.0,
            local: 0.0,
        }
    }

    /// The current time: the game's clock, in seconds.
    #[must_use]
    pub const fn current(&self) -> f64 {
        self.current
    }

    /// The local time: the current time less the server's offset.
    #[must_use]
    pub const fn local(&self) -> f64 {
        self.local
    }

    /// Bring both times up to the host's `elapsed` seconds. The current time is the elapsed time
    /// plus the offset; should that be earlier than the current time already is, it moves on by
    /// 10^-12 s instead.
    // D2-17629
    pub fn update(&mut self, elapsed: f64) {
        let before = self.current;
        let mut now = elapsed + self.base;
        if now < before {
            now = before + 1e-12;
        }
        self.current = now;
        self.local = now - self.base;
    }

    /// The server's time, at the host's `elapsed` seconds: when it is more than 10^-9 s ahead of
    /// this clock, the offset grows by the difference and the current time becomes the server's;
    /// a clock ahead of the server stays as it is. Answers whether the clock moved. The local time
    /// waits for the next [`Timer::update`].
    // D2-17628
    pub fn set(&mut self, server: f64, elapsed: f64) -> bool {
        let ahead = server - (elapsed + self.base);
        if ahead > 1e-9 {
            self.base += ahead;
            self.current = server;
            true
        } else {
            false
        }
    }
}

/// The elapsed time as one thread is answered it: never less than the last answer, unless that was
/// not above zero.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Elapsed {
    last: f64,
}

impl Elapsed {
    /// The answer for the host's `raw` elapsed seconds: `raw`, unless the last answer was above
    /// zero and later, which is answered again.
    // D2-17627
    pub fn answer(&mut self, raw: f64) -> f64 {
        if self.last <= 0.0 || self.last <= raw {
            self.last = raw;
        }
        self.last
    }
}
