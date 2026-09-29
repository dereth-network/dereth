// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Performance/RollingAmountOverHitsTracker.cs
//! `RollingAmountOverHitsTracker`: sum, average and maximum over the last N amounts.

// ACE: RollingAmountOverHitsTracker
/// A ring of the last `hits_being_tracked` amounts.
#[derive(Debug, Clone)]
pub struct RollingAmountOverHitsTracker {
    /// `HitsBeingTracked`.
    pub hits_being_tracked: i64,
    amounts: Vec<f64>,
    next_index: usize,
    rolled_over: bool,
    last_amount: f64,
    sum: f64,
}

impl RollingAmountOverHitsTracker {
    // ACE: RollingAmountOverHitsTracker.RollingAmountOverHitsTracker
    /// `new RollingAmountOverHitsTracker(hitsToTrack)`.
    ///
    /// # Panics
    /// When `hits_to_track` is negative (`OverflowException` allocating the array).
    #[must_use]
    pub fn new(hits_to_track: i64) -> Self {
        let n = usize::try_from(hits_to_track).expect("OverflowException: negative array size");
        Self {
            hits_being_tracked: hits_to_track,
            amounts: vec![0.0; n],
            next_index: 0,
            rolled_over: false,
            last_amount: 0.0,
            sum: 0.0,
        }
    }

    // ACE: RollingAmountOverHitsTracker.LastAmount
    /// `LastAmount`.
    #[must_use]
    pub fn last_amount(&self) -> f64 {
        self.last_amount
    }

    // ACE: RollingAmountOverHitsTracker.TotalAmounts
    /// `TotalAmounts`: the number of amounts in the window.
    #[must_use]
    pub fn total_amounts(&self) -> i64 {
        if self.rolled_over {
            return self.hits_being_tracked;
        }
        i64::try_from(self.next_index).unwrap_or(i64::MAX)
    }

    // ACE: RollingAmountOverHitsTracker.Sum
    /// `Sum`, maintained incrementally.
    #[must_use]
    pub fn sum(&self) -> f64 {
        self.sum
    }

    // ACE: RollingAmountOverHitsTracker.AverageAmount
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

    // ACE: RollingAmountOverHitsTracker.LargestAmount
    /// `LargestAmount`: a scan of the window.
    #[must_use]
    pub fn largest_amount(&self) -> f64 {
        let total = usize::try_from(self.total_amounts()).unwrap_or(0);
        if total == 0 {
            return 0.0;
        }

        let mut largest = f64::MIN;

        for &amount in &self.amounts[..total] {
            if amount > largest {
                largest = amount;
            }
        }

        largest
    }

    // ACE: RollingAmountOverHitsTracker.RegisterAmount
    /// Replaces the oldest amount with `amount`.
    ///
    /// # Panics
    /// When the tracker holds zero hits (`IndexOutOfRangeException`).
    pub fn register_amount(&mut self, amount: f64) {
        self.last_amount = amount;

        self.sum -= self.amounts[self.next_index];
        self.amounts[self.next_index] = amount;
        self.sum += self.amounts[self.next_index];

        self.next_index += 1;

        if i64::try_from(self.next_index).unwrap_or(i64::MAX) == self.hits_being_tracked {
            self.next_index = 0;
            self.rolled_over = true;
        }
    }
}
