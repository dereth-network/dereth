//! The clock: where the client's time comes from, the one frame limiter, and the timer every
//! consumer reads once a frame.
//!
//! `Clock` is the time source and `Pacer` the frame limiter; `SystemClock` is both on the
//! windowed path, and `FixedStepClock` is the headless run's simulated time source. `Timer` is
//! the retail timer's arithmetic over whichever clock it is handed.
//!
//! The local zone is the host's: `localtime`'s answer on Windows needs the OS calendar, which is
//! not reachable from here, so the host installs its answer (`install_local_utc_offset`) and both
//! clocks report it. With nothing installed a date is formatted in UTC, which is also what the
//! host's own answer is when it cannot tell.

use std::time::Duration;

/// Where the client's time comes from.
///
/// Three readings and nothing else: the monotonic one `Timer` is driven by, the wall-clock one
/// the six `localtime` surfaces and the two RNG seeds read, and the local zone's shift for a given
/// instant (which is `dereth_desktop::platform::clock::local_utc_offset_secs`).
///
/// `App` holds one of these as a `Box<dyn Clock>` and hands it to the timer update once
/// per frame. That update takes the only monotonic reading, and every consumer reads
/// the same current time. Sampling the clock more than once per frame changes physics results.
pub trait Clock {
    /// Seconds since the clock was created, from the high-resolution counter the client samples
    /// once per frame.
    fn monotonic_secs(&self) -> f64;

    /// `timeGetTime()` -- a millisecond counter that **wraps**, which is why
    /// `dereth_render::window_proc::frame_sleep_ms` subtracts with `wrapping_sub`.
    fn tick_ms(&self) -> u32;

    /// `time(NULL)`, unrounded and undecided: `None` when the host clock is before the epoch,
    /// which is not representable and is not a case this build has to render.
    ///
    /// The raw reading rather than a number, because the five sites that take it each carry their
    /// own retail fallback -- `0` for the chat stamp and the ban notice, `1` for both RNG seeds --
    /// and those are the CRT's, not this module's.
    fn unix_time(&self) -> Option<Duration>;

    /// `time(NULL)` as the ban notice and the chat stamp take it: seconds, `0` when the host
    /// cannot answer.
    fn unix_secs(&self) -> i64 {
        self.unix_time()
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
    }

    /// The shift from UTC that the CRT's `localtime` would apply to `at`.
    ///
    /// Required, with no default body: the natural default,
    /// `dereth_desktop::platform::clock::local_utc_offset_secs`, is the
    /// `windows::Globalization::Calendar` call, and a default body naming the `windows` crate
    /// would tie this trait to `dereth-client`. Both implementations call that function; an
    /// implementation that wants UTC has to say so.
    fn utc_offset_secs(&self, at: i64) -> i32;

    /// The simulated step a `--headless` run advances by, or `None` on the
    /// windowed path, which reads the real clock. See
    /// `dereth_client_runtime::platform::clock::FixedStepClock`.
    fn fixed_step(&self) -> Option<f64> {
        None
    }
}

/// **the only frame limiter there is**, and inventing another
/// changes the timestep and therefore the physics.
pub trait Pacer {
    /// Sleep this frame's share and return the milliseconds slept, for the recorder.
    fn frame_sleep(&mut self, is_active_app: bool) -> u32;
}

/// Read wall-clock Unix time, leaving each caller's fallback and integer conversion to that caller.
#[must_use]
pub fn system_unix_time() -> Option<Duration> {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .ok()
}

/// The host's answer to "what shift from UTC would the C runtime's `localtime` apply to this
/// instant", in seconds — installed once by the host (the platform owns the zone database), read
/// by the model wherever the client formats a local date: the chat stamp, the house panel, the
/// fellowship and the `@loadfile` date.
static LOCAL_UTC_OFFSET: std::sync::OnceLock<fn(i64) -> i32> = std::sync::OnceLock::new();

/// Install the host's local-offset answer. The first installation stands; later ones are the
/// same function and are ignored.
pub fn install_local_utc_offset(f: fn(i64) -> i32) {
    let _ = LOCAL_UTC_OFFSET.set(f);
}

/// The shift from UTC the host's `localtime` would apply to `unix_secs`, in seconds; `0` (UTC)
/// before the host has installed its answer, which is what the host's own answer is when it
/// cannot tell.
#[must_use]
pub fn local_utc_offset_secs(unix_secs: i64) -> i32 {
    LOCAL_UTC_OFFSET.get().map_or(0, |f| f(unix_secs))
}

/// The simulated step a headless run advances by, one per frame.
///
/// 1/30 s is exactly [`dereth_physics::globals::MIN_QUANTUM`], so every headless frame opens the
/// physics gate once and the state after *n* frames is the state after *n* sub-steps. It is **not**
/// a frame rate and it is **not** the gate: see [`Clock::fixed_step`].
pub const HEADLESS_STEP: f64 = dereth_physics::globals::MIN_QUANTUM;

use web_time::Instant;

use dereth_client_contract::window_proc::frame_sleep_ms;

/// System clock: monotonic elapsed time, wall-clock Unix time, and frame pacing through
/// the platform sleep adapter.
#[derive(Debug)]
pub struct SystemClock {
    start: Instant,
    /// The frame-sleep stamp -- `timeGetTime()` of the previous frame pace.
    last_frame_tick_ms: u32,
}

impl SystemClock {
    /// Start the system clock from the current monotonic `Instant`
    /// and initialize the last-frame tick to zero.
    #[must_use]
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            last_frame_tick_ms: 0,
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn monotonic_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn tick_ms(&self) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the deliberate 32-bit wrap `timeGetTime` has, not a float conversion.
        {
            self.start.elapsed().as_millis() as u32
        }
    }

    fn unix_time(&self) -> Option<Duration> {
        system_unix_time()
    }

    /// The host's zone for that instant, through the answer the host installed.
    fn utc_offset_secs(&self, at: i64) -> i32 {
        local_utc_offset_secs(at)
    }
}

impl Pacer for SystemClock {
    /// Returns the milliseconds slept, for the recorder.
    ///
    /// Active: `Sleep(0)` — a bare yield, no frame limiter. Inactive: frames are spaced at least
    /// 99 ms apart. **There is no other frame limiter**, and inventing one changes the timestep and
    /// therefore the physics.
    fn frame_sleep(&mut self, is_active_app: bool) -> u32 {
        let ms = frame_sleep_ms(is_active_app, self.tick_ms(), self.last_frame_tick_ms);
        if ms == 0 {
            std::thread::yield_now();
        } else {
            std::thread::sleep(Duration::from_millis(u64::from(ms)));
        }
        self.last_frame_tick_ms = self.tick_ms();
        ms
    }
}

/// **Rebuild-only, and only for `--headless`.** The clock whose monotonic reading advances by a
/// fixed step instead of following the wall clock.
///
/// This is not a frame-rate cap and it is not the 30 Hz physics gate: it is what makes the
/// headless capture a *regression test*. The character is stepped by elapsed time, so a capture
/// taken after one wall-clock frame would show the body wherever that machine's scheduler left it.
/// The windowed path uses `SystemClock` and reads the real clock, exactly as
/// the client does.
///
/// It implements `Clock` and **not** `Pacer`: a headless run still paces through a
/// `SystemClock`, because the frame pace's `Sleep(0)` is a yield and not a simulated quantity.
/// [`Self::monotonic_secs`] is still the wall clock, because under a fixed
/// step, local time deliberately remains real so network cadence is independent of frame rate.
#[derive(Debug)]
pub struct FixedStepClock {
    start: Instant,
    step: f64,
}

impl FixedStepClock {
    #[must_use]
    pub fn new(step: f64) -> Self {
        Self {
            start: Instant::now(),
            step,
        }
    }
}

impl Clock for FixedStepClock {
    fn monotonic_secs(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }

    fn tick_ms(&self) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: the deliberate 32-bit wrap `timeGetTime` has, not a float conversion.
        {
            self.start.elapsed().as_millis() as u32
        }
    }

    fn unix_time(&self) -> Option<Duration> {
        system_unix_time()
    }

    fn fixed_step(&self) -> Option<f64> {
        Some(self.step)
    }

    /// The host's zone for that instant, through the answer the host installed.
    fn utc_offset_secs(&self, at: i64) -> i32 {
        local_utc_offset_secs(at)
    }
}

/// The `1e-09` second dead band used by both server-time corrections.
///
/// It is not a tolerance on *accepting* a sync -- that is the `external_time < S` gate -- it is
/// the band inside which an accepted sync changes nothing, so a sync that lands within a nanosecond
/// of the clock leaves `external_offset` exactly as it was.
pub const EXTERNAL_TIME_EPSILON: f64 = 1e-09;

/// `Timer` — the one clock, sampled once per frame.
///
/// The timer is sampled once per frame and every consumer reads that sample. Sampling the clock
/// more than once per frame changes physics results.
#[derive(Debug, Default)]
pub struct Timer {
    /// Server-adjusted current time, a double published from [`Self::external_time`] and never
    /// written directly.
    pub cur_time: f64,
    /// Local current time, published from [`Self::elapsed_time`].
    pub local_time: f64,
    /// Server time minus local time.
    ///
    /// **Zero until a `TimeSync` datagram arrives**, and the only thing that ever changes it is
    /// [`Self::set_time`]. That is what keeps a `--headless` run deterministic without the clock
    /// having to special-case the network: with no sync the offset stays 0, so `cur_time` *is*
    /// `elapsed_time` and every fixed-step capture is reproducible.
    external_offset: f64,
    /// The monotonic local watermark.
    ///
    /// Frame-time update raises it and never lowers it; the **only** thing that can move it
    /// backwards is [`Self::set_time`]'s second correction branch.
    elapsed_time: f64,
    /// `elapsed_time + external_offset`, the server-adjusted time.
    external_time: f64,
}

impl Timer {
    /// Initialize all timer state to zero, including elapsed and server-adjusted time.
    /// The caller supplies a clock when updating this state.
    #[must_use]
    pub fn init() -> Self {
        Self {
            cur_time: 0.0,
            local_time: 0.0,
            // The constructor zeroes the offset, the elapsed time and the external time in that
            // order, and zeroes both globals.
            external_offset: 0.0,
            elapsed_time: 0.0,
            external_time: 0.0,
        }
    }

    /// Compute elapsed time from the performance counter.
    ///
    /// Under `FixedStepClock` there is no time source to sample: the simulated clock advances
    /// only in [`Self::update_time`], so a resample taken between two frames must return the same
    /// reading -- which is what the real function returns when no wall time has passed, via its
    /// last-sample watermark. This is what stops [`Self::set_time`] stealing a step.
    fn compute_elapsed_time(&self, clock: &dyn Clock) -> f64 {
        match clock.fixed_step() {
            Some(_) => self.elapsed_time,
            None => clock.monotonic_secs(),
        }
    }

    /// The final two publication stores:
    /// `local_time = elapsed_time; cur_time = external_time`.
    ///
    /// The one deviation is `local_time` under `FixedStepClock`, and it is deliberate -- see
    /// the note in [`Self::update_time`].
    fn publish(&mut self, clock: &dyn Clock) {
        self.local_time = match clock.fixed_step() {
            Some(_) => clock.monotonic_secs(),
            None => self.elapsed_time,
        };
        self.cur_time = self.external_time;
    }

    /// Step 1 of the frame timer update: sample the elapsed time; if it did not move past the
    /// previous `elapsed_time`, keep the previous value, otherwise store it and set
    /// `external_time = t + external_offset`; then publish `local_time` and `cur_time`.
    ///
    /// **This is the only thing that moves the clock forward.** Server synchronization can only change
    /// the *offset*, or in its second branch pull `elapsed_time` back; it never advances it.
    pub fn update_time(&mut self, clock: &dyn Clock) {
        let prev = self.elapsed_time;
        let t = match clock.fixed_step() {
            Some(step) => prev + step,
            None => self.compute_elapsed_time(clock),
        };
        self.elapsed_time = t;
        if t <= prev {
            self.elapsed_time = prev;
        } else {
            self.external_time = t + self.external_offset;
        }
        // `local_time` is the **raw** timer: frame update writes it from
        // `elapsed_time` and `cur_time` from `external_time`, so the two are equal only while
        // the server correction is zero. The distinction is dead weight in an offline slice and
        // load-bearing in a connected one: the network's cadences (including the 2 s poll,
        // the connect-response resend's 0.333333333 s, the 140 s no-data timeout) all run off
        // `local_time`, and `--headless` deliberately steps `cur_time` by a fixed quantum so a
        // capture is reproducible. Stepping the *network's* clock with it would make a login
        // attempt's timeout depend on the frame rate. So under `fixed_step` -- and only then --
        // `local_time` reads the wall clock while `elapsed_time` and `cur_time` are simulated.
        self.publish(clock);
    }

    /// Apply an authoritative server time.
    ///
    /// This is not an unconditional hard set. It first compares `S` with the existing external
    /// time and returns after publishing when `S <= external_time`; correction is forward-gated.
    ///
    /// Inside the gate it re-runs `update_time`'s watermark step and then corrects, using `1e-09`
    /// on both sides. If the server is more than `1e-09` ahead, the offset becomes
    /// `S - elapsed_time` and `external_time` becomes `S` (`cur_time` jumps, `local_time` is
    /// untouched). If instead the timer overshot `S` by more than `1e-09` between syncs,
    /// `elapsed_time` is pulled back by the overshoot and `external_time` becomes `S` (`local_time`
    /// goes **backwards**, `cur_time` is smooth).
    ///
    /// The second branch is reachable despite the gate because the gate tests the *old*
    /// `external_time` and the watermark step then advances it past `S`.
    ///
    /// Its only production caller is the `TimeSync` handler, which is why a missing synchronization
    /// leaves the map's date and day/night cycle on a process-local epoch.
    pub fn set_time(&mut self, clock: &dyn Clock, server_time: f64) {
        let s = server_time;
        if self.external_time < s {
            let prev = self.elapsed_time;
            let t = self.compute_elapsed_time(clock);
            self.elapsed_time = t;
            if t <= prev {
                self.elapsed_time = prev;
            } else {
                self.external_time = t + self.external_offset;
            }
            if self.external_time + EXTERNAL_TIME_EPSILON < s {
                self.external_offset = s - self.elapsed_time;
                self.external_time = s;
            } else if s + EXTERNAL_TIME_EPSILON < self.external_time {
                self.elapsed_time -= self.external_time - s;
                self.external_time = s;
            }
        }
        self.publish(clock);
    }

    /// `external_offset` -- server time minus local time. `0.0` means "never synchronised".
    #[must_use]
    pub fn external_offset(&self) -> f64 {
        self.external_offset
    }

    /// `elapsed_time` -- the monotonic local watermark `local_time` is published from.
    #[must_use]
    pub fn elapsed_time(&self) -> f64 {
        self.elapsed_time
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FixedStepClock, Pacer as _, SystemClock, Timer, EXTERNAL_TIME_EPSILON, HEADLESS_STEP,
    };
    use dereth_client_contract::window_proc::frame_sleep_ms;
    use std::time::Duration;

    // Oracle: the verified device/window frame-pacing behavior.
    #[test]
    fn the_frame_pace_is_the_documented_one_and_nothing_else() {
        let mut c = SystemClock::new();
        // Active: Sleep(0), a bare yield.
        assert_eq!(c.frame_sleep(true), 0);
        // Inactive with a fresh tick: at most 99 ms, and the wrap is the original's.
        assert_eq!(frame_sleep_ms(false, 0, 0), 99);
        assert_eq!(frame_sleep_ms(false, 50, 0), 49);
        assert_eq!(frame_sleep_ms(false, 99, 0), 0);
        assert_eq!(frame_sleep_ms(false, 200, 0), 0);
        // "The original subtracts two DWORDs, so the arithmetic wraps rather than saturating."
        assert_eq!(
            frame_sleep_ms(false, 9, u32::MAX),
            89,
            "9 - 0xFFFFFFFF wraps to 10"
        );
        assert_eq!(
            frame_sleep_ms(true, 0, 0),
            0,
            "the foreground has no frame limiter at all"
        );
    }

    #[test]
    fn the_clock_is_sampled_once_and_both_globals_agree() {
        let clock = SystemClock::new();
        let mut c = Timer::init();
        c.update_time(&clock);
        let t = c.cur_time;
        assert_eq!(c.local_time, t);
        assert!(t >= 0.0);
    }

    /// `--headless` simulates `cur_time` and leaves `local_time` on the wall clock, because the
    /// network's cadences run off `local_time` and must not be a function of the frame rate.
    /// Oracle: the timer update writes `local_time` from the raw timer and derives `cur_time` from
    /// the server-adjusted value.
    #[test]
    fn a_fixed_step_moves_only_the_simulated_clock() {
        let clock = FixedStepClock::new(HEADLESS_STEP);
        let mut c = Timer::init();
        for i in 1..=5 {
            c.update_time(&clock);
            assert!((c.cur_time - HEADLESS_STEP * f64::from(i)).abs() < 1e-12);
        }
        assert!(
            c.local_time < c.cur_time,
            "five frames do not take a sixth of a second"
        );
    }

    /// The forward-only gate and the fact that the forward correction moves the
    /// *offset* and leaves `elapsed_time` alone.
    #[test]
    fn set_time_accepts_a_later_server_time_by_moving_only_the_offset() {
        let clock = FixedStepClock::new(HEADLESS_STEP);
        let mut c = Timer::init();
        c.update_time(&clock);
        let local = c.elapsed_time;

        c.set_time(&clock, 1_000.0);
        assert_eq!(c.cur_time, 1_000.0, "cur_time is the server's time exactly");
        assert_eq!(
            c.elapsed_time, local,
            "elapsed_time is untouched by a forward correction"
        );
        assert_eq!(
            c.external_offset,
            1_000.0 - local,
            "external_offset = S - elapsed_time"
        );

        // And the step still drives it afterwards: cur_time = elapsed + offset.
        c.update_time(&clock);
        assert!((c.cur_time - (1_000.0 + HEADLESS_STEP)).abs() < 1e-9);
    }

    /// `S <= external_time` -> fall through to the publishing stores only.
    #[test]
    fn set_time_rejects_an_earlier_or_equal_server_time_outright() {
        let clock = FixedStepClock::new(HEADLESS_STEP);
        let mut c = Timer::init();
        c.update_time(&clock);
        c.set_time(&clock, 1_000.0);
        let (offset, elapsed) = (c.external_offset, c.elapsed_time);

        for stale in [999.999_999, 500.0, 0.0, -1.0, 1_000.0] {
            c.set_time(&clock, stale);
            assert_eq!(
                c.cur_time, 1_000.0,
                "a sync at {stale} must not move cur_time"
            );
            assert_eq!(c.external_offset, offset, "nor external_offset");
            assert_eq!(c.elapsed_time, elapsed, "nor elapsed_time");
        }
    }

    /// **The second `1e-09` branch, which the gate does not make unreachable.**
    ///
    /// The gate tests the *old* `external_time`; the watermark step then advances it, and if
    /// that lands past `S`, the correction runs backward as
    /// `elapsed_time -= external_time - S`. This is the **only** path on which local time goes
    /// backward, and the whole reason the two globals are not
    /// interchangeable: `cur_time` lands smoothly on `S` while `local_time` is rewound.
    ///
    /// It needs the wall-clock time source, because under a fixed step `compute_elapsed_time` never
    /// returns more than `elapsed_time` and so can never overshoot. Set up as the timer instance
    /// is laid out -- `elapsed_time` behind the real reading, a large `external_offset` -- and
    /// then a sync that clears the old external time but not the advanced one.
    #[test]
    fn set_time_pulls_the_local_clock_back_when_the_timer_overshoots_the_server() {
        let clock = SystemClock::new();
        let mut c = Timer::init();
        c.elapsed_time = 0.0;
        c.external_offset = 1_000.0;
        c.external_time = 1_000.0; // elapsed + offset, as `update_time` would leave it

        // Let the real time source get ahead of `elapsed_time`.
        std::thread::sleep(Duration::from_millis(4));
        let t = c.compute_elapsed_time(&clock);
        assert!(t > 1e-6, "the wall clock moved: {t}");

        // Strictly between `prev + offset` (1000.0) and `t + offset`, so the gate admits it and the
        // advanced `external_time` then overshoots it.
        let s = 1_000.0 + t / 2.0;
        c.set_time(&clock, s);

        assert_eq!(c.external_time, s, "external_time = S");
        assert_eq!(c.cur_time, s, "published");
        assert_eq!(
            c.external_offset, 1_000.0,
            "a backward correction leaves the offset alone"
        );
        // `elapsed = t' - ((t' + 1000) - (1000 + t/2)) = t/2`, for any `t'` the resample returns --
        // the extra microseconds between the sample above and the one inside cancel exactly.
        assert!(
            (c.elapsed_time - t / 2.0).abs() < 1e-6,
            "elapsed_time was pulled back to t/2: {} vs {}",
            c.elapsed_time,
            t / 2.0
        );
        assert!(
            c.local_time < t,
            "local_time went backwards, which only this branch can do"
        );
    }

    /// Both `1e-09` dead bands: accepted by the gate, corrected by neither branch.
    #[test]
    fn a_sync_inside_the_dead_band_is_accepted_and_corrects_nothing() {
        let clock = FixedStepClock::new(0.0);
        let mut c = Timer::init();
        c.elapsed_time = 100.0;
        c.external_offset = 900.0;
        c.external_time = 1_000.0;

        c.set_time(&clock, 1_000.0 + EXTERNAL_TIME_EPSILON / 2.0);
        assert_eq!(
            c.external_offset, 900.0,
            "no forward correction inside the band"
        );
        assert_eq!(c.elapsed_time, 100.0, "no backward correction either");
        assert_eq!(
            c.external_time, 1_000.0,
            "external_time is not even set to S"
        );
        assert_eq!(c.cur_time, 1_000.0, "and the publishing stores still ran");
    }
}
