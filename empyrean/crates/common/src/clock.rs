//! Injected clocks. Nothing in the server reads the OS clock except
//! [`SystemClock`]; a test drives [`VirtualClock`] and advances it by any amount instantly.
//!
//! ACE reads three clocks:
//! * **UTC date-time** (`DateTime.UtcNow`): [`Clock::utc_now`];
//! * **unix time** in `double` seconds (`Time.GetUnixTime()`): [`Clock::unix_time`];
//! * **game time** (`Timers.PortalYearTicks`), `double` seconds that the world loop advances by the
//!   elapsed time of each `UpdateWorld` iteration. It is world state, seeded from the start time
//!   (`Timers.WorldStartLoreTime`, see [`DerethDateTime::utc_now_to_emu_time`]) and advanced from
//!   [`Clock::monotonic`] deltas; the world captures it in a [`ClockSnapshot`] once per tick.
//!
//! [`Clock::monotonic`] is also the time base of [`Stopwatch`] (`System.Diagnostics.Stopwatch`).
//!
//! [`DerethDateTime::utc_now_to_emu_time`]: crate::dereth_date_time::DerethDateTime::utc_now_to_emu_time

use std::fmt;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::dotnet::datetime::{DotNetDateTime, TimeSpan};
use crate::time::Time;

/// A source of the current time.
pub trait Clock: Send + Sync + fmt::Debug {
    /// `DateTime.UtcNow`.
    fn utc_now(&self) -> DotNetDateTime;

    /// Monotonic time since an arbitrary, fixed origin (the `Stopwatch` time base).
    fn monotonic(&self) -> Duration;

    /// `Time.GetUnixTime()`: seconds since 1970-01-01 as a `double`.
    fn unix_time(&self) -> f64 {
        Time::get_unix_time_at(self.utc_now())
    }
}

/// Converts a `Duration` to whole 100 ns ticks, truncating (as `Stopwatch.Elapsed` does).
#[must_use]
pub fn duration_to_ticks(d: Duration) -> i64 {
    i64::try_from(d.as_nanos() / 100).unwrap_or(i64::MAX)
}

/// The operating system's clocks. The only type in the server that reads them.
#[derive(Debug)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// A system clock whose monotonic origin is now.
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn utc_now(&self) -> DotNetDateTime {
        let ticks = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => duration_to_ticks(d),
            Err(e) => -duration_to_ticks(e.duration()),
        };
        DotNetDateTime::UNIX_EPOCH.add_ticks(ticks)
    }

    fn monotonic(&self) -> Duration {
        self.origin.elapsed()
    }
}

#[derive(Debug, Clone, Copy)]
struct VirtualState {
    utc: DotNetDateTime,
    monotonic: Duration,
}

/// A clock that moves only when told to. Shareable across threads (`Arc<VirtualClock>`).
#[derive(Debug)]
pub struct VirtualClock {
    state: Mutex<VirtualState>,
}

impl VirtualClock {
    /// A virtual clock reading `utc`, with its monotonic time at zero.
    #[must_use]
    pub fn new(utc: DotNetDateTime) -> Self {
        Self {
            state: Mutex::new(VirtualState {
                utc,
                monotonic: Duration::ZERO,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VirtualState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Moves both the UTC and the monotonic time forward by `by` (UTC to 100 ns resolution).
    pub fn advance(&self, by: Duration) {
        let mut s = self.lock();
        s.utc = s.utc.add_ticks(duration_to_ticks(by));
        s.monotonic += by;
    }

    /// Sets the UTC time without touching the monotonic time (a wall-clock jump).
    pub fn set_utc(&self, utc: DotNetDateTime) {
        self.lock().utc = utc;
    }
}

impl Default for VirtualClock {
    /// A virtual clock at 2026-01-01 00:00:00 UTC.
    fn default() -> Self {
        Self::new(DotNetDateTime::new(2026, 1, 1))
    }
}

impl Clock for VirtualClock {
    fn utc_now(&self) -> DotNetDateTime {
        self.lock().utc
    }

    fn monotonic(&self) -> Duration {
        self.lock().monotonic
    }
}

/// The three clocks as read once at the start of a world tick; everything inside the tick uses
/// these values (as ACE uses the frozen `Timers.PortalYearTicks` within one `UpdateWorld`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockSnapshot {
    /// `Timers.PortalYearTicks`: game time in seconds.
    pub portal_year_ticks: f64,
    /// `Time.GetUnixTime()`.
    pub unix_time: f64,
    /// `DateTime.UtcNow`.
    pub utc: DotNetDateTime,
    /// The monotonic reading the snapshot was taken at.
    pub monotonic: Duration,
}

impl ClockSnapshot {
    /// Reads `clock` once. `portal_year_ticks` is the world's game time, which the world owns.
    #[must_use]
    pub fn take(clock: &dyn Clock, portal_year_ticks: f64) -> Self {
        let utc = clock.utc_now();
        Self {
            portal_year_ticks,
            unix_time: Time::get_unix_time_at(utc),
            utc,
            monotonic: clock.monotonic(),
        }
    }
}

/// A [`Clock`] frozen at a tick's [`ClockSnapshot`], for code inside the tick that takes a clock
/// (the stopwatches, rate monitors and rate limiters): every read returns the snapshot's UTC and
/// monotonic values. `unix_time` keeps the trait's default, derived from the snapshot's UTC, rather
/// than reading the snapshot's own `unix_time` field.
#[derive(Debug, Clone, Copy)]
pub struct SnapshotClock(pub ClockSnapshot);

impl Clock for SnapshotClock {
    fn utc_now(&self) -> DotNetDateTime {
        self.0.utc
    }

    fn monotonic(&self) -> Duration {
        self.0.monotonic
    }
}

/// `System.Diagnostics.Stopwatch` over an injected [`Clock`]'s monotonic time.
#[derive(Debug, Clone, Copy, Default)]
pub struct Stopwatch {
    accumulated: Duration,
    started_at: Option<Duration>,
}

impl Stopwatch {
    /// `new Stopwatch()`: stopped, zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `Stopwatch.StartNew()`.
    #[must_use]
    pub fn start_new(clock: &dyn Clock) -> Self {
        let mut s = Self::new();
        s.start(clock);
        s
    }

    /// `Start()`: no effect when already running.
    pub fn start(&mut self, clock: &dyn Clock) {
        if self.started_at.is_none() {
            self.started_at = Some(clock.monotonic());
        }
    }

    /// `Stop()`: no effect when already stopped.
    pub fn stop(&mut self, clock: &dyn Clock) {
        if let Some(start) = self.started_at.take() {
            self.accumulated += clock.monotonic().saturating_sub(start);
        }
    }

    /// `Reset()`: stopped, zero.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `Restart()`: zero, running.
    pub fn restart(&mut self, clock: &dyn Clock) {
        *self = Self::start_new(clock);
    }

    /// `IsRunning`.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.started_at.is_some()
    }

    /// `Elapsed`, truncated to 100 ns ticks.
    #[must_use]
    pub fn elapsed(&self, clock: &dyn Clock) -> TimeSpan {
        let running = self
            .started_at
            .map_or(Duration::ZERO, |s| clock.monotonic().saturating_sub(s));
        TimeSpan::from_ticks(duration_to_ticks(self.accumulated + running))
    }
}
