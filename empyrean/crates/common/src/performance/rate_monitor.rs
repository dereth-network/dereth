// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Performance/RateMonitor.cs
//! `RateMonitor`: a stopwatch that feeds a [`TimedEventHistory`].

use std::fmt;

use super::timed_event_history::TimedEventHistory;
use crate::clock::{Clock, Stopwatch};

// ACE: RateMonitor
/// Times events and keeps their history.
#[derive(Debug, Clone, Default)]
pub struct RateMonitor {
    stopwatch: Stopwatch,
    /// `EventHistory`.
    pub event_history: TimedEventHistory,
}

impl RateMonitor {
    /// `new RateMonitor()`: stopped.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: RateMonitor.Reset
    /// Stops time interval measurement and resets the elapsed time to zero.
    pub fn reset(&mut self) {
        self.stopwatch.reset();
    }

    // ACE: RateMonitor.Restart
    /// Resets the elapsed time to zero and starts measuring.
    pub fn restart(&mut self, clock: &dyn Clock) {
        self.stopwatch.restart(clock);
    }

    // ACE: RateMonitor.Pause
    /// Stops time interval measurement.
    pub fn pause(&mut self, clock: &dyn Clock) {
        self.stopwatch.stop(clock);
    }

    // ACE: RateMonitor.Resume
    /// Starts time interval measurement.
    pub fn resume(&mut self, clock: &dyn Clock) {
        self.stopwatch.start(clock);
    }

    // ACE: RateMonitor.RegisterEventEnd
    /// Stops the stopwatch and records its elapsed time as one event.
    pub fn register_event_end(&mut self, clock: &dyn Clock) {
        self.stopwatch.stop(clock);

        let seconds = self.stopwatch.elapsed(clock).total_seconds();
        self.register_event(seconds);
    }

    // ACE: RateMonitor.RegisterEvent
    /// Records an event of `total_seconds`.
    pub fn register_event(&mut self, total_seconds: f64) {
        self.event_history.register_event(total_seconds);
    }

    // ACE: RateMonitor.ClearEventHistory
    /// Clears the history (not the stopwatch).
    pub fn clear_event_history(&mut self) {
        self.event_history.clear_history();
    }
}

impl fmt::Display for RateMonitor {
    // ACE: RateMonitor.ToString
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.event_history.fmt(f)
    }
}
