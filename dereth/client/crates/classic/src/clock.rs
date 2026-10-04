//! Elapsed UI time from the clock supplied by the host.
use dereth_primitives::LocalTime;

pub(crate) fn seconds(now: LocalTime, since: LocalTime) -> f64 {
    now.seconds_since(since).max(0.0)
}

pub(crate) fn milliseconds(now: LocalTime, since: LocalTime) -> u128 {
    // Round to nanosecond precision before truncating to whole milliseconds.
    std::time::Duration::try_from_secs_f64(seconds(now, since))
        .map_or(u128::MAX, |elapsed| elapsed.as_millis())
}
