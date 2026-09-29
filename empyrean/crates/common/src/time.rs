// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Time.cs
//! `ACE.Common.Time`, plus the clock abstraction every other time read goes through
//! ([`crate::clock`], re-exported here).
//!
//! DIVERGE: ACE's parameterless members read `DateTime.UtcNow`; here they take the injected
//! [`Clock`]. The arithmetic is unchanged.

pub use crate::clock::{
    duration_to_ticks, Clock, ClockSnapshot, SnapshotClock, Stopwatch, SystemClock, VirtualClock,
};
use crate::dotnet::datetime::DotNetDateTime;

/// `ACE.Common.Time` (a class with only static members in ACE).
#[derive(Debug)]
pub struct Time;

impl Time {
    // ACE: Time.unixEpoch
    /// `new DateTime(1970, 1, 1, 0, 0, 0, 0, DateTimeKind.Utc)`.
    pub const UNIX_EPOCH: DotNetDateTime = DotNetDateTime::UNIX_EPOCH;

    // ACE: Time.GetUnixTime()
    /// `GetUnixTime(DateTime.UtcNow)`, with the clock injected.
    #[must_use]
    pub fn get_unix_time(clock: &dyn Clock) -> f64 {
        Self::get_unix_time_at(clock.utc_now())
    }

    // ACE: Time.GetUnixTime(DateTime)
    /// `(dateTime - unixEpoch).TotalSeconds`. (C# overload `GetUnixTime(DateTime)`.)
    #[must_use]
    pub fn get_unix_time_at(date_time: DotNetDateTime) -> f64 {
        let span = date_time - Self::UNIX_EPOCH;
        span.total_seconds()
    }

    // ACE: Time.GetFutureUnixTime
    /// `(DateTime.UtcNow.AddSeconds(seconds) - unixEpoch).TotalSeconds`, with the clock injected.
    #[must_use]
    pub fn get_future_unix_time(clock: &dyn Clock, seconds: f64) -> f64 {
        let span = clock.utc_now().add_seconds(seconds) - Self::UNIX_EPOCH;
        span.total_seconds()
    }

    // ACE: Time.GetDateTimeFromTimestamp
    /// `unixEpoch.AddSeconds(timestamp)`.
    #[must_use]
    pub fn get_date_time_from_timestamp(timestamp: f64) -> DotNetDateTime {
        Self::UNIX_EPOCH.add_seconds(timestamp)
    }
}
