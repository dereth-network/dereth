// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Performance/RateLimiter.cs
//! `RateLimiter`: spaces events evenly over a period.

use crate::clock::{Clock, Stopwatch};
use crate::dotnet::TimeSpan;

// ACE: RateLimiter
/// At most `max_number_of_events` per `over_period`, spaced evenly.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    max_number_of_events: i32,
    over_period_in_seconds: f64,
    target_event_spacing_in_seconds: f64,
    stopwatch: Stopwatch,
    number_of_events_registered: i32,
}

impl RateLimiter {
    // ACE: RateLimiter.RateLimiter
    /// `new RateLimiter(maxNumberOfEvents, overPeriod)`; the stopwatch starts now.
    ///
    /// # Panics
    /// When `max_number_of_events <= 0` or `over_period <= TimeSpan.Zero`
    /// (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn new(max_number_of_events: i32, over_period: TimeSpan, clock: &dyn Clock) -> Self {
        assert!(
            max_number_of_events > 0,
            "ArgumentOutOfRangeException: maxNumberOfEvents must be greater than 0"
        );
        assert!(
            over_period > TimeSpan::ZERO,
            "ArgumentOutOfRangeException: overPeriod must be greater than TimeSpan.Zero"
        );

        let over_period_in_seconds = over_period.total_seconds();
        Self {
            max_number_of_events,
            over_period_in_seconds,
            target_event_spacing_in_seconds: over_period_in_seconds
                / f64::from(max_number_of_events),
            stopwatch: Stopwatch::start_new(clock),
            number_of_events_registered: 0,
        }
    }

    // ACE: RateLimiter.GetSecondsToWaitBeforeNextEvent
    /// Positive: wait this long. Zero: on time. Negative: behind; register without delay.
    #[must_use]
    pub fn get_seconds_to_wait_before_next_event(&self, clock: &dyn Clock) -> f64 {
        let elapsed_seconds = self.stopwatch.elapsed(clock).total_seconds();

        (self.target_event_spacing_in_seconds * f64::from(self.number_of_events_registered))
            - elapsed_seconds
    }

    // ACE: RateLimiter.RegisterEvent
    /// Counts one event; restarts the window once the count or the period is exceeded.
    pub fn register_event(&mut self, clock: &dyn Clock) {
        self.number_of_events_registered += 1;

        let elapsed_seconds = self.stopwatch.elapsed(clock).total_seconds();

        if self.number_of_events_registered > self.max_number_of_events
            || elapsed_seconds > self.over_period_in_seconds
        {
            self.number_of_events_registered = 1;

            self.stopwatch.restart(clock);
        }
    }
}
