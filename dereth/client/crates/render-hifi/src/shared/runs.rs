//! Runs of the frame's recorded commands a pass draws in place of, in the order the recording
//! made them, and whether a command falls in one: asked of every draw each replay goes through,
//! so it is answered by a search of the runs, not by reading them all. At the longest draw
//! distance a frame's landscape is thousands of runs.

use std::cell::Cell;
use std::ops::Range;

/// Runs of command indices, each after the one before and none overlapping.
#[derive(Debug, Default)]
pub struct Runs {
    runs: Vec<Range<u32>>,
    /// How many runs the questions so far have looked at.
    looked: Cell<u64>,
}

impl Runs {
    /// No runs, and nothing looked at.
    pub fn clear(&mut self) {
        self.runs.clear();
        self.looked.set(0);
    }

    /// Add `run`, which starts at or after the end of the last one.
    pub fn push(&mut self, run: Range<u32>) {
        debug_assert!(
            self.runs.last().is_none_or(|last| last.end <= run.start),
            "runs are added in the recording's order"
        );
        self.runs.push(run);
    }

    /// How many runs there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Whether command `cmd` falls in a run.
    #[must_use]
    pub fn contains(&self, cmd: u32) -> bool {
        // The first run that ends after `cmd`; the halvings are the runs looked at.
        let (mut lo, mut hi) = (0, self.runs.len());
        let mut looked = 0;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            looked += 1;
            if self.runs[mid].end <= cmd {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        self.looked.set(self.looked.get() + looked + 1);
        self.runs.get(lo).is_some_and(|r| r.contains(&cmd))
    }

    /// How many runs the questions since the runs were last cleared have looked at.
    #[must_use]
    pub fn looked(&self) -> u64 {
        self.looked.get()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi presentation)
    use super::*;

    /// The longest draw distance's landscape: thousands of cells, each a run of a few commands
    /// with other draws between, and every command of the recording asked about, as a replay
    /// asks.
    #[test]
    fn a_command_is_found_in_a_few_looks_however_many_runs_there_are() {
        let mut runs = Runs::default();
        let cells = 2601u32;
        for k in 0..cells {
            runs.push(k * 5 + 1..k * 5 + 3);
        }
        let commands = cells * 5;
        let inside = (0..commands).filter(|&c| runs.contains(c)).count();
        assert_eq!(inside, 2 * cells as usize);
        assert!(!runs.contains(0) && runs.contains(1) && runs.contains(2) && !runs.contains(3));
        let per = runs.looked() as f64 / f64::from(commands);
        assert!(
            per <= 14.0,
            "{} runs looked at for {commands} commands among {cells} runs, {per:.1} each",
            runs.looked()
        );
        runs.clear();
        assert_eq!(runs.looked(), 0);
        assert!(!runs.contains(1));
    }
}
