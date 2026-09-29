// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Performance/TimedEventHistory.cs
//! `TimedEventHistory`: count, total, longest, shortest and last event duration.

use std::fmt;

use crate::dotnet::format;

// ACE: TimedEventHistory
/// Durations in seconds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TimedEventHistory {
    /// `LastEvent`: the last event's duration.
    pub last_event: f64,
    /// `TotalEvents`.
    pub total_events: i64,
    /// `TotalSeconds`.
    pub total_seconds: f64,
    /// `LongestEvent`.
    pub longest_event: f64,
    /// `ShortestEvent`.
    pub shortest_event: f64,
}

impl TimedEventHistory {
    /// `new TimedEventHistory()`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: TimedEventHistory.AverageEventDuration
    /// `TotalSeconds / TotalEvents`, or 0 with no events.
    #[must_use]
    pub fn average_event_duration(&self) -> f64 {
        if self.total_events == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let n = self.total_events as f64;
            self.total_seconds / n
        }
    }

    // ACE: TimedEventHistory.RegisterEvent
    /// Records one event of `total_seconds`.
    pub fn register_event(&mut self, total_seconds: f64) {
        self.last_event = total_seconds;

        self.total_events += 1;
        self.total_seconds += self.last_event;

        if self.last_event > self.longest_event {
            self.longest_event = self.last_event;
        }

        if self.total_events == 1 || self.last_event < self.shortest_event {
            self.shortest_event = self.last_event;
        }
    }

    // ACE: TimedEventHistory.ClearHistory
    /// Resets every field to zero.
    pub fn clear_history(&mut self) {
        self.last_event = 0.0;
        self.total_events = 0;
        self.total_seconds = 0.0;
        self.longest_event = 0.0;
        self.shortest_event = 0.0;
    }
}

impl fmt::Display for TimedEventHistory {
    // ACE: TimedEventHistory.ToString
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Total Events: {}, Average: {} s, Longest: {} s, Shortest: {} s, Last: {} s",
            format(self.total_events, "N0"),
            format(self.average_event_duration(), "N4"),
            format(self.longest_event, "N4"),
            format(self.shortest_event, "N4"),
            format(self.last_event, "N4")
        )
    }
}
