//! Vectors: fixtures/vectors/performance/
//! ServerPerformanceMonitor report on virtual clock matches ACE's; switches and histories.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use std::sync::Arc;
use std::time::Duration;

use empyrean_common::clock::{Clock, ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::{format, TimeSpan};
use empyrean_common::vectors;
use empyrean_dat::FakeDats;
use empyrean_world::managers::server_performance_monitor::{
    self as perf, CumulativeEventHistoryType, MonitorType,
};
use empyrean_world::World;

/// `PerformanceVectors.MonitorTicks`.
fn monitor_ticks(i: usize, k: usize, phase: usize) -> i64 {
    let (i, k, phase) = (i as i64, k as i64, phase as i64);
    if i == 4 {
        123_456_789 * (k + 1)
    } else {
        (i + 1) * 12_500 + k * 3_000 + phase * 1_000
    }
}

/// `PerformanceVectors.CumulativeTicks`.
fn cumulative_ticks(j: usize, phase: usize) -> i64 {
    (j as i64 + 1) * 40_000 + phase as i64 * 7
}

fn ticks(t: i64) -> Duration {
    Duration::from_nanos(u64::try_from(t).expect("positive") * 100)
}

fn world(clock: &VirtualClock) -> World {
    let now = ClockSnapshot::take(clock, 0.0);
    World::new(now, FakeDats::new().build().expect("fake dats"))
}

/// The run the vectors script, on the world's hooks: every monitored stage is timed by the
/// virtual clock moving between `RestartEvent` and `RegisterEventEnd`, the cumulative amounts are
/// added twice and registered, and five minutes pass between the phases so that the next `Tick`
/// clears the ~5m windows only. Returns the minutes each window has run.
fn scripted_run(w: &mut World, clock: &Arc<VirtualClock>) -> (f64, f64) {
    perf::use_clock(w, Arc::<VirtualClock>::clone(clock));
    perf::start(w);
    // The first Tick clears everything: the clear times start at DateTime.MinValue.
    perf::tick(w);
    let start = clock.utc_now();
    let mut last_5m_clear = start;

    for phase in 0..2 {
        if phase == 1 {
            clock.advance(Duration::from_secs(300));
            perf::tick(w);
            last_5m_clear = clock.utc_now();
        }
        for (i, &t) in MonitorType::ALL.iter().enumerate() {
            for k in 0..=(i % 3) {
                perf::restart_event(w, t);
                clock.advance(ticks(monitor_ticks(i, k, phase)));
                perf::register_event_end(w, t);
            }
        }
        perf::restart_cumulative_events(w);
        for (j, &t) in CumulativeEventHistoryType::ALL.iter().enumerate() {
            let s = TimeSpan::from_ticks(cumulative_ticks(j, phase)).total_seconds();
            perf::add_to_cumulative_event(w, t, s);
            perf::add_to_cumulative_event(w, t, s);
        }
        perf::register_cumulative_events(w);
        perf::tick(w);
    }
    let now = clock.utc_now();
    (
        (now - last_5m_clear).total_minutes(),
        (now - start).total_minutes(),
    )
}

/// The `serverperformance` report after the scripted run equals ACE's, with and without the
/// cumulative monitors, and its first line gives how long each window has run.
#[test]
fn report_after_a_scripted_run_matches_ace() {
    let file = vectors::load_named("performance", "server_performance_monitor_to_string");
    assert_eq!(file.cases.len(), 2);
    for c in &file.cases {
        let cumulative = c.input["cumulative"].as_bool().expect("cumulative");
        let clock = Arc::new(VirtualClock::default());
        let mut w = world(&clock);
        let (m5, m1) = scripted_run(&mut w, &clock);
        if !cumulative {
            perf::stop_cumulative(&mut w);
        }
        let text = perf::to_string(&w);
        let (first, rest) = text.split_at(text.find('\n').expect("lines") + 1);
        assert_eq!(
            rest,
            c.output["text"].as_str().expect("text"),
            "cumulative {cumulative}"
        );
        assert_eq!(
            first,
            format!(
                "Monitoring Durations: ~5m {} min, ~1h {} min, ~24h {} min\n",
                format(m5, "N2"),
                format(m1, "N2"),
                format(m1, "N2")
            )
        );
    }
}

/// `GetEventHistory*` and the switches: nothing is measured until the monitor starts, `Stop`
/// freezes the histories, a start after a stop keeps them (ACE's `Reset` inside `Start` runs
/// while still stopped), and `Reset` on a running monitor clears every window.
#[test]
fn switches_and_histories() {
    let clock = Arc::new(VirtualClock::default());
    let mut w = world(&clock);
    perf::use_clock(&mut w, Arc::<VirtualClock>::clone(&clock));
    let t = MonitorType::UpdateGameWorldEntire;

    perf::restart_event(&mut w, t);
    clock.advance(Duration::from_millis(5));
    perf::register_event_end(&mut w, t);
    assert_eq!(
        perf::get_event_history_5m(&w, t).total_events,
        0,
        "not running"
    );

    perf::start(&mut w);
    assert!(w.performance.is_running && w.performance.is_running_cumulative);
    perf::tick(&mut w);
    for ms in [5u64, 15] {
        perf::restart_event(&mut w, t);
        clock.advance(Duration::from_millis(ms));
        perf::register_event_end(&mut w, t);
    }
    for h in [
        perf::get_event_history_5m(&w, t),
        perf::get_event_history_1h(&w, t),
        perf::get_event_history_24h(&w, t),
    ] {
        assert_eq!(h.total_events, 2);
        assert!((h.average_event_duration() - 0.010).abs() < 1e-12);
        assert!((h.longest_event - 0.015).abs() < 1e-12);
        assert!((h.last_event - 0.015).abs() < 1e-12);
    }

    perf::stop(&mut w);
    assert!(!w.performance.is_running && !w.performance.is_running_cumulative);
    perf::restart_event(&mut w, t);
    clock.advance(Duration::from_millis(5));
    perf::register_event_end(&mut w, t);
    perf::start(&mut w);
    assert_eq!(
        perf::get_event_history_5m(&w, t).total_events,
        2,
        "Start after Stop keeps the histories"
    );

    // The ~1h window clears an hour after its last clear; the ~24h one does not.
    clock.advance(Duration::from_secs(3600));
    perf::tick(&mut w);
    assert_eq!(perf::get_event_history_5m(&w, t).total_events, 0);
    assert_eq!(perf::get_event_history_1h(&w, t).total_events, 0);
    assert_eq!(perf::get_event_history_24h(&w, t).total_events, 2);

    perf::reset(&mut w);
    assert_eq!(
        perf::get_event_history_24h(&w, t).total_events,
        0,
        "Reset while running clears all"
    );

    // Cumulative events need both switches.
    perf::stop_cumulative(&mut w);
    perf::restart_cumulative_events(&mut w);
    perf::add_to_cumulative_event(
        &mut w,
        CumulativeEventHistoryType::LandblockTickHeartbeat,
        1.0,
    );
    perf::register_cumulative_events(&mut w);
    assert!(
        !perf::to_string(&w).contains("Landblock_Tick_Heartbeat"),
        "the cumulative section is hidden"
    );
}

mod world_timing {
    use crate::support::clock_and_object_lifetime::*;

    /// The cumulative stopwatch reads the monitors clock.
    #[test]
    fn the_cumulative_stopwatch_reads_the_monitors_clock() {
        let mut w = world();
        let clock = Arc::new(VirtualClock::default());
        perf::use_clock(&mut w, clock.clone());
        let start = perf::stopwatch_start(&w);
        clock.advance(Duration::from_millis(250));
        assert!((perf::stopwatch_elapsed_seconds(&w, start) - 0.25).abs() < 1e-9);
        assert_eq!(clock.monotonic() - start, Duration::from_millis(250));
    }

    /// A landblocks rate monitors read the monitors clock.
    #[test]
    fn a_landblocks_rate_monitors_read_the_monitors_clock() {
        let mut w = world();
        empyrean_testkit::land::use_flat_land_with_test_setup(&mut w, &[0xA9B4], 0);
        perf::use_clock(&mut w, Arc::new(SteppingClock::default()));
        let id = LandblockId::new(0xA9B4_FFFF);
        let id =
            empyrean_world::managers::landblock_manager::get_landblock(&mut w, id, false, false);
        let t = w.now.unix_time;
        // (the first tick's end clears the histories: their last clear is DateTime.MinValue)
        for _ in 0..2 {
            empyrean_world::entity::landblock::tick_multi_threaded_work(&mut w, id, t);
            empyrean_world::entity::landblock::tick_single_threaded_work(&mut w, id, t);
        }
        let l = w.landblock_manager.landblocks.get(id).expect("loaded");
        for h in [&l.monitor_5m.event_history, &l.monitor_1h.event_history] {
            assert_eq!(h.total_events, 1);
            assert!(h.last_event > 0.0, "the tick was timed: {}", h.last_event);
        }
    }

    /// Handler timing is off by default and times each invocation when on.
    #[test]
    fn handler_timing_is_off_by_default_and_times_each_invocation_when_on() {
        use empyrean_world::network::managers::inbound_message_manager::{
            self as imm, HandlerTime,
        };
        let mut w = world();
        perf::use_clock(&mut w, Arc::new(SteppingClock::default()));
        let session = empyrean_net::SessionId {
            client_id: 1,
            generation: 1,
        };
        imm::invoke(&mut w, "GameAction", 0x36, "UseItem", session, |_| Ok(()));
        assert!(
            w.sessions.inbound.handler_timing.is_none(),
            "off: nothing recorded"
        );

        w.sessions.inbound.handler_timing = Some(Vec::new());
        imm::invoke(&mut w, "GameAction", 0x36, "UseItem", session, |_| Ok(()));
        imm::invoke(&mut w, "GameMessage", 0xF7B1, "GameAction", session, |_| {
            Err(dereth_protocol::MessageError::UnexpectedEof {
                at: 0,
                needed: 4,
                available: 0,
            })
        });
        let times = w.sessions.inbound.handler_timing.take().expect("on");
        assert_eq!(
            times.iter().map(|t| (t.kind, t.name)).collect::<Vec<_>>(),
            [("GameAction", "UseItem"), ("GameMessage", "GameAction")]
        );
        assert!(
            times
                .iter()
                .all(|t: &HandlerTime| (t.seconds - 0.001).abs() < 1e-9),
            "one clock step each: {times:?}"
        );
        assert_eq!(w.sessions.inbound.handler_exceptions, 1);
    }
}
