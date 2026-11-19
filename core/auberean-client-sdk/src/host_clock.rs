//! A native host's clock: the system's steady clock.

use std::time::Instant;

use crate::driver::Clock;

/// The seconds since the clock was made, on the system's steady clock (it never runs back).
#[derive(Clone, Copy, Debug)]
pub struct HostClock {
    start: Instant,
}

impl HostClock {
    /// A clock at zero now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for HostClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for HostClock {
    fn frame_time(&mut self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}
