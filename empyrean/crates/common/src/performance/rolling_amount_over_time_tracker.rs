// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Performance/RollingAmountOverTimeTracker.cs
//! `RollingAmountOverTimeTracker`: sum, average and maximum over the amounts registered in the
//! last `TimeBeingTracked`.

use std::collections::VecDeque;

use crate::dotnet::datetime::{DotNetDateTime, TimeSpan};

// ACE: RollingAmountOverTimeTracker
/// A time-stamped window of amounts, oldest first.
#[derive(Debug, Clone)]
pub struct RollingAmountOverTimeTracker {
    /// `TimeBeingTracked`.
    pub time_being_tracked: TimeSpan,
    /// `amounts` (a `LinkedList<Tuple<DateTime, double>>`).
    amounts: VecDeque<(DotNetDateTime, f64)>,
    last_amount: f64,
    sum: f64,
}

impl RollingAmountOverTimeTracker {
    // ACE: RollingAmountOverTimeTracker.RollingAmountOverTimeTracker
    /// `new RollingAmountOverTimeTracker(timeToTrack)`.
    #[must_use]
    pub fn new(time_to_track: TimeSpan) -> Self {
        Self {
            time_being_tracked: time_to_track,
            amounts: VecDeque::new(),
            last_amount: 0.0,
            sum: 0.0,
        }
    }

    /// `LastAmount`.
    #[must_use]
    pub fn last_amount(&self) -> f64 {
        self.last_amount
    }

    // ACE: RollingAmountOverTimeTracker.TotalAmounts
    /// `TotalAmounts`: the number of amounts in the window.
    #[must_use]
    pub fn total_amounts(&self) -> i64 {
        i64::try_from(self.amounts.len()).unwrap_or(i64::MAX)
    }

    /// `Sum`, maintained incrementally.
    #[must_use]
    pub fn sum(&self) -> f64 {
        self.sum
    }

    // ACE: RollingAmountOverTimeTracker.AverageAmount
    /// `AverageAmount`: `Sum / TotalAmounts`, or 0.
    #[must_use]
    pub fn average_amount(&self) -> f64 {
        let total = self.total_amounts();
        if total == 0 {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let n = total as f64;
            self.sum / n
        }
    }

    // ACE: RollingAmountOverTimeTracker.LargestAmount
    /// `LargestAmount`: a scan of the window ("use this sparingly").
    #[must_use]
    pub fn largest_amount(&self) -> f64 {
        if self.total_amounts() == 0 {
            return 0.0;
        }

        let mut largest = f64::MIN;

        for &(_, amount) in &self.amounts {
            if amount > largest {
                largest = amount;
            }
        }

        largest
    }

    // ACE: RollingAmountOverTimeTracker.RegisterAmount
    /// Prunes the amounts older than `now - TimeBeingTracked`, then appends `amount`. `now` is
    /// ACE's `DateTime.UtcNow` (the caller reads its injected clock).
    pub fn register_amount(&mut self, amount: f64, now: DotNetDateTime) {
        self.last_amount = amount;

        let prune_before = now - self.time_being_tracked;

        while let Some(&(first_time, first_amount)) = self.amounts.front() {
            if first_time >= prune_before {
                break;
            }
            self.sum -= first_amount;

            self.amounts.pop_front();
        }

        self.sum += amount;

        self.amounts.push_back((now, amount));
    }
}
