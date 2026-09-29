//! ACE: Source/ACE.Common/Performance/RateLimiter.cs::RateLimiter
//! RateLimiter, RateMonitor, TimedEventHistory and the rolling trackers follow ACE on a virtual
//! clock; SHA2 hash hex; EnumHelper.GetFlags order.
//! Fixture: locally constructed values and deterministic expected results.

use std::time::Duration;

use empyrean_common::dotnet::TimeSpan;
use empyrean_common::performance::rate_limiter::RateLimiter;
use empyrean_common::performance::rate_monitor::RateMonitor;
use empyrean_common::performance::rolling_amount_over_hits_tracker::RollingAmountOverHitsTracker;
use empyrean_common::performance::timed_event_history::TimedEventHistory;
use empyrean_common::time::VirtualClock;

#[test]
fn rate_limiter_one_per_minute_as_house_manager_uses_it() {
    let clock = VirtualClock::default();
    let mut rl = RateLimiter::new(1, TimeSpan::from_minutes(1.0), &clock);
    // No event yet: (60 * 0) - 0.
    assert_eq!(rl.get_seconds_to_wait_before_next_event(&clock), 0.0);
    rl.register_event(&clock);
    // One event: (60 * 1) - elapsed.
    assert_eq!(rl.get_seconds_to_wait_before_next_event(&clock), 60.0);
    clock.advance(Duration::from_secs(30));
    assert_eq!(rl.get_seconds_to_wait_before_next_event(&clock), 30.0);
    clock.advance(Duration::from_secs(31));
    assert_eq!(
        rl.get_seconds_to_wait_before_next_event(&clock),
        -1.0,
        "behind: go now"
    );
    // Two events exceed the maximum: the count restarts at 1 and so does the stopwatch.
    rl.register_event(&clock);
    assert_eq!(rl.get_seconds_to_wait_before_next_event(&clock), 60.0);
}

#[test]
fn rate_limiter_restarts_when_the_period_has_passed() {
    let clock = VirtualClock::default();
    let mut rl = RateLimiter::new(10, TimeSpan::from_seconds(1.0), &clock);
    rl.register_event(&clock);
    rl.register_event(&clock);
    // Spacing 0.1 s: (0.1 * 2) - 0.
    assert_eq!(rl.get_seconds_to_wait_before_next_event(&clock), 0.2);
    clock.advance(Duration::from_secs(2));
    // Third event, but 2 s > 1 s: count back to 1, stopwatch restarted.
    rl.register_event(&clock);
    assert_eq!(rl.get_seconds_to_wait_before_next_event(&clock), 0.1);
}

#[test]
#[should_panic(expected = "maxNumberOfEvents")]
fn rate_limiter_rejects_zero_events() {
    let _ = RateLimiter::new(0, TimeSpan::from_seconds(1.0), &VirtualClock::default());
}

#[test]
#[should_panic(expected = "overPeriod")]
fn rate_limiter_rejects_an_empty_period() {
    let _ = RateLimiter::new(1, TimeSpan::ZERO, &VirtualClock::default());
}

#[test]
fn rate_monitor_times_events_into_its_history() {
    let clock = VirtualClock::default();
    let mut m = RateMonitor::new();
    m.restart(&clock);
    clock.advance(Duration::from_millis(250));
    m.register_event_end(&clock);
    assert_eq!(m.event_history.last_event, 0.25);
    // Resume continues the same stopwatch; RegisterEventEnd records its whole elapsed time.
    m.resume(&clock);
    clock.advance(Duration::from_millis(500));
    m.pause(&clock);
    clock.advance(Duration::from_secs(9));
    m.register_event_end(&clock);
    assert_eq!(m.event_history.last_event, 0.75);
    m.register_event(1.25);
    assert_eq!(
        m.to_string(),
        "Total Events: 3, Average: 0.7500 s, Longest: 1.2500 s, Shortest: 0.2500 s, Last: 1.2500 s"
    );
    m.reset();
    m.register_event_end(&clock);
    assert_eq!(
        m.event_history.last_event, 0.0,
        "Reset zeroes the stopwatch"
    );
    m.clear_event_history();
    assert_eq!(m.event_history, TimedEventHistory::new());
}

#[test]
fn timed_event_history_tracks_count_total_extremes() {
    let mut h = TimedEventHistory::new();
    assert_eq!(h.average_event_duration(), 0.0);
    for s in [3.0, 1.0, 2.0] {
        h.register_event(s);
    }
    assert_eq!(
        (h.last_event, h.total_events, h.total_seconds),
        (2.0, 3, 6.0)
    );
    assert_eq!(
        (
            h.longest_event,
            h.shortest_event,
            h.average_event_duration()
        ),
        (3.0, 1.0, 2.0)
    );
    for _ in 0..1231 {
        h.register_event(0.000_05);
    }
    assert!(h.to_string().starts_with("Total Events: 1,234, Average: 0.0049 s, Longest: 3.0000 s, Shortest: 0.0001 s, Last: 0.0001 s"), "{h}");
    h.clear_history();
    assert_eq!(h, TimedEventHistory::default());
}

#[test]
fn rolling_amount_over_hits_tracker_keeps_the_last_n() {
    let mut t = RollingAmountOverHitsTracker::new(3);
    assert_eq!(
        (t.total_amounts(), t.average_amount(), t.largest_amount()),
        (0, 0.0, 0.0)
    );
    t.register_amount(1.0);
    t.register_amount(2.0);
    assert_eq!(
        (
            t.total_amounts(),
            t.sum(),
            t.average_amount(),
            t.largest_amount()
        ),
        (2, 3.0, 1.5, 2.0)
    );
    t.register_amount(3.0);
    t.register_amount(4.0);
    // The ring now holds [4, 2, 3].
    assert_eq!(
        (
            t.total_amounts(),
            t.sum(),
            t.average_amount(),
            t.largest_amount(),
            t.last_amount()
        ),
        (3, 9.0, 3.0, 4.0, 4.0)
    );
    assert_eq!(t.hits_being_tracked, 3);
    let mut neg = RollingAmountOverHitsTracker::new(2);
    neg.register_amount(-5.0);
    assert_eq!(neg.largest_amount(), -5.0, "starts from double.MinValue");
}

#[test]
#[should_panic(expected = "index out of bounds")]
fn rolling_amount_over_zero_hits_throws_on_register() {
    RollingAmountOverHitsTracker::new(0).register_amount(1.0);
}

#[test]
fn rolling_amount_over_time_tracker_prunes_amounts_older_than_its_window() {
    use empyrean_common::clock::Clock;
    use empyrean_common::performance::rolling_amount_over_time_tracker::RollingAmountOverTimeTracker;

    let clock = VirtualClock::default();
    let mut t = RollingAmountOverTimeTracker::new(TimeSpan::from_minutes(1.0));
    assert_eq!(
        (t.total_amounts(), t.average_amount(), t.largest_amount()),
        (0, 0.0, 0.0)
    );
    t.register_amount(10.0, clock.utc_now());
    clock.advance(Duration::from_secs(30));
    t.register_amount(20.0, clock.utc_now());
    assert_eq!(
        (
            t.total_amounts(),
            t.sum(),
            t.average_amount(),
            t.largest_amount()
        ),
        (2, 30.0, 15.0, 20.0)
    );
    // 60 s after the first: `first < now - 1 min` is false at exactly the boundary, so it stays.
    clock.advance(Duration::from_secs(30));
    t.register_amount(3.0, clock.utc_now());
    assert_eq!((t.total_amounts(), t.sum()), (3, 33.0));
    // One tick later the first amount is older than the window and is pruned before the append.
    clock.advance(Duration::from_nanos(100));
    t.register_amount(-4.0, clock.utc_now());
    assert_eq!(
        (
            t.total_amounts(),
            t.sum(),
            t.largest_amount(),
            t.last_amount()
        ),
        (3, 19.0, 20.0, -4.0)
    );
    let mut neg = RollingAmountOverTimeTracker::new(TimeSpan::from_minutes(1.0));
    neg.register_amount(-5.0, clock.utc_now());
    assert_eq!(neg.largest_amount(), -5.0, "starts from double.MinValue");
}
