//! `ACE.Common.Performance`. The `Stopwatch` fields read the injected
//! [`Clock`](crate::clock::Clock), which every time-dependent member takes as a parameter.

pub mod rate_limiter;
pub mod rate_monitor;
pub mod rolling_amount_over_hits_tracker;
pub mod rolling_amount_over_time_tracker;
pub mod timed_event_history;
