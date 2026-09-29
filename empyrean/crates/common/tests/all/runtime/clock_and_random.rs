//! Vectors: local virtual-clock, stopwatch and seeded-random cases in this module
//! Virtual clock moves only when told, Time/Stopwatch follow the injected clock and .NET
//! semantics, thread-local random is deterministic unseeded and per-thread seeded.
//! Fixture: an injected virtual clock and seeded random generators.

use std::time::Duration;

use empyrean_common::dotnet::{DotNetDateTime, TimeSpan};
use empyrean_common::random::{self, DotNetRandom, DEFAULT_SEED};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::time::{
    duration_to_ticks, Clock, ClockSnapshot, Stopwatch, SystemClock, Time, VirtualClock,
};

#[test]
fn virtual_clock_moves_only_when_told() {
    let start = DotNetDateTime::new_hms(2024, 1, 5, 0, 7, 9);
    let clock = VirtualClock::new(start);
    assert_eq!(clock.utc_now(), start);
    assert_eq!(clock.monotonic(), Duration::ZERO);
    clock.advance(Duration::from_millis(1500));
    assert_eq!(clock.utc_now().ticks() - start.ticks(), 15_000_000);
    assert_eq!(clock.monotonic(), Duration::from_millis(1500));
    // A wall-clock jump leaves the monotonic time alone.
    clock.set_utc(DotNetDateTime::new(2030, 1, 1));
    assert_eq!(clock.utc_now(), DotNetDateTime::new(2030, 1, 1));
    assert_eq!(clock.monotonic(), Duration::from_millis(1500));
    // A day in one step, instantly.
    clock.advance(Duration::from_secs(86_400));
    assert_eq!(clock.utc_now(), DotNetDateTime::new(2030, 1, 2));
    assert_eq!(
        VirtualClock::default().utc_now(),
        DotNetDateTime::new(2026, 1, 1)
    );
}

#[test]
fn unix_time_and_time_helpers_use_the_injected_clock() {
    let clock = VirtualClock::new(DotNetDateTime::new_hms_ms(2024, 1, 5, 0, 7, 9, 45));
    // (2024-01-05T00:07:09.045Z - 1970-01-01).TotalSeconds, computed as ticks / 1e7.
    assert_eq!(clock.unix_time(), 1_704_413_229.045);
    assert_eq!(Time::get_unix_time(&clock), clock.unix_time());
    assert_eq!(Time::get_future_unix_time(&clock, 1.5), 1_704_413_230.545);
    assert_eq!(Time::UNIX_EPOCH, DotNetDateTime::new(1970, 1, 1));
    assert_eq!(Time::get_date_time_from_timestamp(0.0), Time::UNIX_EPOCH);
}

#[test]
fn system_clock_reads_the_os() {
    let clock = SystemClock::new();
    assert!(clock.utc_now() > DotNetDateTime::new(2024, 1, 1));
    let a = clock.monotonic();
    let b = clock.monotonic();
    assert!(b >= a);
    assert!(clock.unix_time() > 1.7e9);
    let _ = SystemClock::default();
}

#[test]
fn clock_snapshot_reads_once() {
    let clock = VirtualClock::new(DotNetDateTime::new(2024, 1, 1));
    clock.advance(Duration::from_secs(2));
    let snap = ClockSnapshot::take(&clock, 123.5);
    clock.advance(Duration::from_secs(10));
    assert_eq!(snap.portal_year_ticks, 123.5);
    assert_eq!(snap.utc, DotNetDateTime::new_hms(2024, 1, 1, 0, 0, 2));
    assert_eq!(snap.unix_time, Time::get_unix_time_at(snap.utc));
    assert_eq!(snap.monotonic, Duration::from_secs(2));
}

#[test]
fn stopwatch_follows_dotnet_semantics() {
    let clock = VirtualClock::default();
    let mut sw = Stopwatch::new();
    assert!(!sw.is_running());
    assert_eq!(sw.elapsed(&clock), TimeSpan::ZERO);
    sw.start(&clock);
    clock.advance(Duration::from_millis(250));
    assert_eq!(sw.elapsed(&clock), TimeSpan::from_milliseconds(250.0));
    sw.stop(&clock);
    clock.advance(Duration::from_secs(5));
    assert_eq!(
        sw.elapsed(&clock),
        TimeSpan::from_milliseconds(250.0),
        "stopped: no time accrues"
    );
    sw.start(&clock);
    sw.start(&clock);
    clock.advance(Duration::from_millis(750));
    assert_eq!(
        sw.elapsed(&clock).total_seconds(),
        1.0,
        "resuming accumulates"
    );
    sw.restart(&clock);
    assert!(sw.is_running());
    assert_eq!(sw.elapsed(&clock), TimeSpan::ZERO);
    clock.advance(Duration::from_nanos(150));
    assert_eq!(
        sw.elapsed(&clock).ticks(),
        1,
        "Elapsed truncates to 100 ns ticks"
    );
    sw.reset();
    assert!(!sw.is_running());
    let sw = Stopwatch::start_new(&clock);
    clock.advance(Duration::from_secs(3));
    assert_eq!(sw.elapsed(&clock).total_seconds(), 3.0);
    assert_eq!(duration_to_ticks(Duration::from_nanos(1_999)), 19);
}

fn default_sequence(n: usize) -> Vec<i32> {
    let mut r = DotNetRandom::new(DEFAULT_SEED);
    (0..n).map(|_| r.next_range(0, 11)).collect()
}

#[test]
fn an_unseeded_thread_is_deterministic() {
    // Each thread starts from DEFAULT_SEED unless something seeded it.
    let run = || {
        std::thread::spawn(|| {
            (0..20)
                .map(|_| ThreadSafeRandom::next(0, 10))
                .collect::<Vec<_>>()
        })
        .join()
        .expect("thread")
    };
    let a = run();
    assert_eq!(a, run());
    assert_eq!(a, default_sequence(20));
}

#[test]
fn seeding_is_per_thread_and_uses_the_low_32_bits() {
    ThreadSafeRandom::seed(0x1_0000_0007);
    let a: Vec<i32> = (0..10).map(|_| ThreadSafeRandom::next(1, 100)).collect();
    random::seed(7);
    let b: Vec<i32> = (0..10).map(|_| ThreadSafeRandom::next(1, 100)).collect();
    assert_eq!(a, b);
    // Another thread is unaffected by this thread's seed.
    let other = std::thread::spawn(|| {
        (0..20)
            .map(|_| ThreadSafeRandom::next(0, 10))
            .collect::<Vec<_>>()
    })
    .join()
    .expect("thread");
    assert_eq!(other, default_sequence(20));
    // with_thread_rng reaches the same generator.
    random::seed(7);
    let first = random::with_thread_rng(DotNetRandom::next);
    assert_eq!(first, DotNetRandom::new(7).next());
}

#[test]
fn seed_from_entropy_is_explicit_and_per_thread() {
    let drawn = std::thread::spawn(|| {
        ThreadSafeRandom::seed_from_entropy();
        (0..20)
            .map(|_| ThreadSafeRandom::next(0, 10))
            .collect::<Vec<_>>()
    })
    .join()
    .expect("thread");
    // 11^20 possible sequences: equal to the default sequence only by a vanishing chance.
    assert_ne!(drawn, default_sequence(20));
    let unaffected = std::thread::spawn(|| {
        (0..20)
            .map(|_| ThreadSafeRandom::next(0, 10))
            .collect::<Vec<_>>()
    })
    .join()
    .expect("thread");
    assert_eq!(unaffected, default_sequence(20));
    random::seed_from_entropy();
}

#[test]
fn thread_safe_random_next_is_inclusive() {
    ThreadSafeRandom::seed(3);
    let draws: Vec<i32> = (0..2000).map(|_| ThreadSafeRandom::next(1, 3)).collect();
    assert!(draws.contains(&1) && draws.contains(&3) && draws.iter().all(|v| (1..=3).contains(v)));
    assert_eq!(ThreadSafeRandom::next(5, 5), 5);
    // ACE-BUG kept: max + 1 wraps to int.MinValue; with min == int.MinValue the range is empty
    // and Random.Next returns min.
    assert_eq!(ThreadSafeRandom::next(i32::MIN, i32::MAX), i32::MIN);
    let f = ThreadSafeRandom::next_float(2.0, 2.0);
    assert_eq!(f, 2.0);
    assert!(ThreadSafeRandom::next_interval(2.0) == 0.0, "clamped at 0");
    assert!(
        ThreadSafeRandom::next_interval_max(2.0) == 0.999_999_999_999_999_9,
        "clamped below 1"
    );
}

#[test]
#[should_panic(expected = "ArgumentOutOfRangeException")]
fn thread_safe_random_next_throws_for_int_max_as_in_ace() {
    // ACE-BUG kept: Next(0, int.MaxValue) calls Random.Next(0, int.MinValue), which throws.
    let _ = ThreadSafeRandom::next(0, i32::MAX);
}

#[test]
#[should_panic(expected = "ArgumentOutOfRangeException")]
fn thread_safe_random_next_throws_when_min_exceeds_max_plus_one() {
    let _ = ThreadSafeRandom::next(3, 1);
}
