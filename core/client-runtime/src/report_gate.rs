//! A rate limit for the console's state lines.
//!
//! Several lines print the client's state whenever it changes: the object census, the physics
//! census, the landscape census, the cursor, requests nothing handled. In a busy area those
//! states change every frame or two, and a console fed hundreds of lines a minute is both
//! unreadable and slow, because a console write blocks the frame that makes it. A
//! `ReportGate` lets such a line out at most once per `REPORT_INTERVAL`.
//!
//! The pattern at a print site is `if changed && gate.ready() { remember; print }`: a change
//! that arrives while the gate is shut is not remembered as printed, so the line goes out with
//! the latest state as soon as the gate opens, and a state that stops changing is always the last
//! one printed.

use std::time::Duration;

use web_time::Instant;

/// The shortest time between two lines through one gate.
pub const REPORT_INTERVAL: Duration = Duration::from_secs(5);

/// One print site's rate limit. The first line always goes out.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReportGate {
    last: Option<Instant>,
}

impl ReportGate {
    /// Whether a line may go out now. `true` also starts the next interval, so call it only when
    /// there is something to print.
    pub fn ready(&mut self) -> bool {
        let now = Instant::now();
        if self
            .last
            .is_some_and(|t| now.duration_since(t) < REPORT_INTERVAL)
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_line_goes_out_and_the_next_waits_for_the_interval() {
        let mut g = ReportGate::default();
        assert!(g.ready());
        assert!(!g.ready());
        g.last = Some(Instant::now() - REPORT_INTERVAL);
        assert!(g.ready());
    }
}
