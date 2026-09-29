//! ACE: Source/ACE.Server/Managers/WorldManager.cs::UpdateWorld
//! UpdateWorld tick order, 60 Hz limiter, PortalYearTicks, inbound ActionQueue,
//! WorldManager/ServerManager members.
//! Fixture: synthetic dats, isolated world state.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::not_ported;
use empyrean_dat::FakeDats;
use empyrean_world::entity::actions::action_chain::ActionChain;
use empyrean_world::entity::actions::i_action::Action;
use empyrean_world::entity::actions::i_actor::{enqueue, Actor};
use empyrean_world::entity::timers::TimersState;
use empyrean_world::managers::server_manager::{self as sm, ShutdownStage};
use empyrean_world::managers::world_manager::{self as wm, NoWire, WorldHost, WorldStatusState};
use empyrean_world::network::managers::inbound_message_manager::run_inbound_message_queue;
use empyrean_world::World;

type Log = Arc<Mutex<Vec<String>>>;

fn note(l: &Log, name: &str) -> impl FnOnce(&mut World) + Send + 'static {
    let (l, name) = (Arc::clone(l), name.to_string());
    move |_w: &mut World| l.lock().unwrap().push(name)
}

fn entries(l: &Log) -> Vec<String> {
    l.lock().unwrap().clone()
}

/// A world on `clock`, with ACE's start-up `Timers`.
fn world_on(clock: &VirtualClock) -> World {
    let timers = TimersState::new(clock);
    let now = ClockSnapshot::take(clock, timers.portal_year_ticks);
    let mut w = World::new(now, FakeDats::new().build().expect("empty fake dats"));
    w.timers = timers;
    w
}

/// One loop iteration at the clock's current time, as the harness runs it.
fn tick(w: &mut World, clock: &VirtualClock) -> wm::UpdateWorldTick {
    let now = ClockSnapshot::take(clock, w.timers.portal_year_ticks);
    w.tick(now, &mut NoWire)
}

/// Advances the clock and `PortalYearTicks` together, as `update_world` does after an iteration.
fn pass(w: &mut World, clock: &VirtualClock, d: Duration) {
    clock.advance(d);
    empyrean_world::entity::timers::advance_portal_year_ticks(
        w,
        TimeSpan::from_ticks(i64::try_from(d.as_nanos() / 100).unwrap()),
    );
}

#[test]
fn one_tick_runs_the_inbound_queue_then_the_world_queue_then_the_delays() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    let l: Log = Arc::default();

    // Enqueued in the reverse of the order UpdateWorld runs them.
    let mut delayed = ActionChain::new();
    delayed
        .add_delay_seconds(&mut w, 0.5)
        .add_action(Actor::World, note(&l, "after delay"));
    delayed.enqueue_chain(&mut w);
    // A delay that is due now: `new DelayAction(0)` (AddDelaySeconds refuses 0).
    let mut due = empyrean_world::entity::actions::delay_action::DelayAction::new(&mut w, 0.0);
    due.base
        .run_on_finish(Actor::World, Action::delegate(note(&l, "due delay's next")));
    enqueue(&mut w, Actor::Delay, Action::Delay(due));
    enqueue(&mut w, Actor::World, Action::delegate(note(&l, "world")));
    enqueue(
        &mut w,
        Actor::InboundMessageQueue,
        Action::delegate(note(&l, "inbound")),
    );

    not_ported::take_local();
    let r = tick(&mut w, &clock);
    assert_eq!(
        entries(&l),
        ["inbound", "world"],
        "the due delay ran after the world queue"
    );
    assert!(
        r.game_world_updated,
        "the first UpdateGameWorld is never rate limited"
    );
    assert_eq!(r.session_count, 0);
    let hits = not_ported::take_local();
    assert!(
        !hits.keys().any(|k| k.starts_with("ACE: HouseManager")),
        "{hits:?}"
    );
    assert!(
        !hits.keys().any(|k| k.starts_with("ACE: PlayerManager")),
        "{hits:?}"
    );
    assert!(
        !hits.keys().any(|k| k.starts_with("ACE: Landblock")),
        "{hits:?}"
    );

    // The due delay's next action went to the world queue during this tick's delay stage, so it
    // runs in the next tick's world stage.
    tick(&mut w, &clock);
    assert_eq!(entries(&l), ["inbound", "world", "due delay's next"]);

    // The half-second delay runs in the first tick at or after its end time.
    pass(&mut w, &clock, Duration::from_millis(499));
    tick(&mut w, &clock);
    assert_eq!(entries(&l).len(), 3);
    pass(&mut w, &clock, Duration::from_millis(1));
    tick(&mut w, &clock); // the delay fires, enqueuing its next action on the world queue
    tick(&mut w, &clock);
    assert_eq!(entries(&l).last().map(String::as_str), Some("after delay"));
}

#[test]
fn work_enqueued_by_a_world_action_waits_for_the_next_tick() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    let l: Log = Arc::default();
    let inner = note(&l, "second tick");
    enqueue(
        &mut w,
        Actor::World,
        Action::delegate(move |w: &mut World| {
            wm::enqueue_action(w, Action::delegate(inner));
        }),
    );
    tick(&mut w, &clock);
    assert!(entries(&l).is_empty());
    tick(&mut w, &clock);
    assert_eq!(entries(&l), ["second tick"]);
}

#[test]
fn inbound_queue_is_an_action_queue_reached_through_its_actor() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    let l: Log = Arc::default();
    let again = note(&l, "requeued");
    enqueue(
        &mut w,
        Actor::InboundMessageQueue,
        Action::delegate(move |w: &mut World| {
            enqueue(w, Actor::InboundMessageQueue, Action::delegate(again))
        }),
    );
    enqueue(
        &mut w,
        Actor::InboundMessageQueue,
        Action::delegate(note(&l, "first")),
    );
    assert_eq!(w.sessions.inbound.inbound_message_queue.len(), 2);
    run_inbound_message_queue(&mut w);
    assert_eq!(entries(&l), ["first"], "RunActions' count snapshot");
    run_inbound_message_queue(&mut w);
    assert_eq!(entries(&l), ["first", "requeued"]);
}

#[test]
fn update_game_world_runs_sixty_times_a_second() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    let mut updated_at = Vec::new();
    for ms in 0..1000u64 {
        if tick(&mut w, &clock).game_world_updated {
            updated_at.push(ms);
        }
        pass(&mut w, &clock, Duration::from_millis(1));
    }
    assert_eq!(updated_at.len(), 60, "{updated_at:?}");
    // Evenly spaced: the n-th update is the first tick at or after n/60 s.
    for (n, ms) in updated_at.iter().enumerate() {
        let due = (n as f64) / 60.0 * 1000.0;
        assert!(
            (*ms as f64) >= due - 1e-6 && (*ms as f64) < due + 1.0,
            "update {n} at {ms} ms"
        );
    }
    // At 1000 ms the 61st event is on time; registering it restarts the window.
    assert!(tick(&mut w, &clock).game_world_updated);
}

/// A host whose `Thread.Sleep` moves the virtual clock, and which stops the loop after `limit`
/// iterations.
struct Host<'a> {
    clock: &'a VirtualClock,
    sleeps: Vec<Duration>,
    iterations: usize,
    limit: usize,
}

impl WorldHost for Host<'_> {
    fn sleep(&mut self, duration: Duration) {
        self.sleeps.push(duration);
        self.clock.advance(duration);
    }

    fn between_iterations(&mut self, w: &mut World) {
        self.iterations += 1;
        if self.iterations == self.limit {
            wm::stop_world(w);
        }
    }
}

#[test]
fn update_world_sleeps_between_updates_and_advances_portal_year_ticks_by_the_loop_time() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    let start = w.timers.portal_year_ticks;
    let mut host = Host {
        clock: &clock,
        sleeps: Vec::new(),
        iterations: 0,
        limit: 5,
    };
    wm::update_world(&mut w, &mut NoWire, &clock, &mut host);

    // Iteration 1 (t = 0) updates the game world and does not sleep; 2 and 3 wait for the limiter
    // and sleep 10 ms each (no sessions); 4 (t = 20 ms, past 1/60 s) updates; 5 sleeps again.
    assert_eq!(host.sleeps, vec![Duration::from_millis(10); 3]);
    assert!(!w.world_manager.world_active);
    assert!(
        (w.timers.portal_year_ticks - (start + 0.030)).abs() < 1e-9,
        "{}",
        w.timers.portal_year_ticks - start
    );
    // The next tick's snapshot carries the advanced game time.
    assert_eq!(
        ClockSnapshot::take(&clock, w.timers.portal_year_ticks).portal_year_ticks,
        w.timers.portal_year_ticks
    );
}

#[test]
fn a_panicking_action_is_caught_and_the_loop_goes_on() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    let l: Log = Arc::default();
    enqueue(
        &mut w,
        Actor::World,
        Action::delegate(|_w: &mut World| panic!("boom")),
    );
    enqueue(
        &mut w,
        Actor::World,
        Action::delegate(note(&l, "behind the panic")),
    );
    enqueue(
        &mut w,
        Actor::InboundMessageQueue,
        Action::delegate(note(&l, "inbound")),
    );

    tick(&mut w, &clock);
    // F14: the action's own guard catches it, so the rest of the queue runs in the same tick and
    // the stage guard is not reached.
    assert_eq!(w.world_manager.stage_exceptions, 0);
    assert_eq!(w.world_manager.action_queue.act_exceptions(), 1);
    assert_eq!(entries(&l), ["inbound", "behind the panic"]);
    tick(&mut w, &clock);
    assert_eq!(entries(&l), ["inbound", "behind the panic"]);
}

#[test]
fn open_and_close_set_the_world_status() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    assert_eq!(w.world_manager.world_status, WorldStatusState::Closed);
    not_ported::take_local();
    wm::open(&mut w, None);
    assert_eq!(w.world_manager.world_status, WorldStatusState::Open);
    wm::close(&mut w, None, true);
    assert_eq!(w.world_manager.world_status, WorldStatusState::Closed);
    let hits = not_ported::take_local();
    // BroadcastToAuditChannel and BootAllPlayers are PlayerManager's own: both call their implementations.
    assert!(
        !hits.keys().any(|k| k.starts_with("ACE: PlayerManager.")),
        "{hits:?}"
    );
}

#[test]
fn append_lines_follows_the_dotnet_regex() {
    assert_eq!(wm::append_lines(&["a", "", "b"]), "a\nb");
    assert_eq!(wm::append_lines(&["", ""]), "");
    // `$` also matches before a final "\n": a line ending in "\n" loses both newlines.
    assert_eq!(wm::append_lines(&["header", "motd\n"]), "header\nmotd");
    assert_eq!(wm::append_lines(&["x\n\n"]), "x\n");
}

// (`ThreadSafeTeleport` now teleports a real player: its test is in movement.rs)

/// Ticks once and polls the shutdown thread, as the server's host does between iterations.
fn tick_and_poll(w: &mut World, clock: &VirtualClock) -> bool {
    tick(w, clock);
    sm::shutdown_server(w)
}

#[test]
fn shutdown_counts_down_then_stops_the_world_and_exits() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    w.world_manager.world_active = true;
    sm::set_shutdown_interval(&mut w, 5);
    tick(&mut w, &clock);
    sm::begin_shutdown(&mut w);
    assert!(w.server_manager.shutdown_initiated);
    let deadline = clock_utc(&clock).add_seconds(5.0);
    assert_eq!(w.server_manager.shutdown_time, deadline);
    assert_eq!(
        w.net.shutdown_time,
        Some(deadline),
        "the transport refuses logins inside 2 minutes"
    );

    // The countdown holds while ShutdownTime >= UtcNow.
    for _ in 0..5 {
        assert!(!tick_and_poll(&mut w, &clock));
        assert_eq!(
            w.server_manager.shutdown_stage(),
            Some(ShutdownStage::Countdown)
        );
        pass(&mut w, &clock, Duration::from_secs(1));
    }
    assert!(!tick_and_poll(&mut w, &clock), "still equal at 5 s: `>=`");
    pass(&mut w, &clock, Duration::from_millis(1));
    assert!(!tick_and_poll(&mut w, &clock));
    assert!(w.server_manager.shutdown_in_progress && w.net.shutdown_in_progress);
    // No players, no authenticated sessions, no landblocks: straight on to StopWorld.
    assert_eq!(
        w.server_manager.shutdown_stage(),
        Some(ShutdownStage::StoppingWorld)
    );
    assert!(w.world_manager.pending_world_stop);

    // The loop ends (update_world clears WorldActive), then the thread finishes.
    w.world_manager.world_active = false;
    assert!(sm::shutdown_server(&mut w));
    assert_eq!(
        w.server_manager.shutdown_stage(),
        Some(ShutdownStage::Exited)
    );
}

#[test]
fn a_cancelled_shutdown_ends_its_thread() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    sm::set_shutdown_interval(&mut w, 60);
    sm::begin_shutdown(&mut w);
    assert!(!tick_and_poll(&mut w, &clock));
    sm::cancel_shutdown(&mut w);
    assert_eq!(w.net.shutdown_time, None);
    assert!(!tick_and_poll(&mut w, &clock));
    assert_eq!(w.server_manager.shutdown_stage(), None);
    assert!(!w.world_manager.pending_world_stop);
}

#[test]
fn do_shutdown_now_skips_the_countdown() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    w.world_manager.world_active = true;
    sm::set_shutdown_interval(&mut w, 60);
    sm::do_shutdown_now(&mut w);
    assert_eq!(w.server_manager.shutdown_interval, 0);
    // ShutdownTime == UtcNow still counts down (`>=`); one clock step later it proceeds.
    assert!(!tick_and_poll(&mut w, &clock));
    assert_eq!(
        w.server_manager.shutdown_stage(),
        Some(ShutdownStage::Countdown)
    );
    pass(&mut w, &clock, Duration::from_millis(1));
    assert!(!tick_and_poll(&mut w, &clock));
    assert_eq!(
        w.server_manager.shutdown_stage(),
        Some(ShutdownStage::StoppingWorld)
    );
}

#[test]
fn pending_shutdown_texts_match_aces_formatting() {
    let s = |secs: f64| sm::pending_shutdown_time_text(TimeSpan::from_seconds(secs));
    assert_eq!(s(90.0), "1 minute and 30 seconds");
    assert_eq!(s(7200.0), "2 hours");
    assert_eq!(s(3600.0 + 300.0 + 1.0), "1 hour, 5 minutes and 1 second");
    assert_eq!(s(3601.0), "1 hour and 1 second");
    assert_eq!(s(10.0), "10 seconds");

    let b = |secs: f64| sm::pending_shutdown_broadcast(TimeSpan::from_seconds(secs), &s(secs));
    assert_eq!(b(1800.0), "Broadcast from System> ATTENTION - This Asheron's Call Server will be shutting down in 30 minutes.");
    assert_eq!(
        b(120.0),
        "Broadcast from System> ATTENTION - This Asheron's Call Server will be shutting down in 2 minutes. Please log out."
    );
    assert_eq!(
        b(60.0),
        "Broadcast from System> WARNING - This Asheron's Call Server will be shutting down in 1 minute! Please log out!"
    );
    assert_eq!(
        b(10.0),
        "Broadcast from System> ATTENTION - This Asheron's Call Server is shutting down NOW!!!!"
    );
}

#[test]
fn shutdown_notice_text_separates_only_the_parts_that_are_there() {
    // V344 (a fix): ACE read "2 minutes and " and "1 hour,  and 5 seconds" here.
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    tick(&mut w, &clock);
    let at = |w: &mut World, secs: f64| {
        w.server_manager.shutdown_time = w.now.utc.add_seconds(secs);
        sm::shutdown_notice_text(w)
    };
    assert_eq!(
        at(&mut w, 120.0),
        "Broadcast from System> ATTENTION - This Asheron's Call Server will be shutting down in 2 minutes. Please log out."
    );
    assert_eq!(
        at(&mut w, 3605.0),
        "Broadcast from System> ATTENTION - This Asheron's Call Server will be shutting down in 1 hour and 5 seconds."
    );
    assert_eq!(
        at(&mut w, 3600.0 + 300.0 + 1.0),
        "Broadcast from System> ATTENTION - This Asheron's Call Server will be shutting down in 1 hour, 5 minutes and 1 second."
    );
    assert_eq!(
        at(&mut w, 7200.0),
        "Broadcast from System> ATTENTION - This Asheron's Call Server will be shutting down in 2 hours."
    );
    assert_eq!(
        at(&mut w, 45.0),
        "Broadcast from System> WARNING - This Asheron's Call Server will be shutting down in 45 seconds! Please log out!"
    );
}

fn clock_utc(clock: &VirtualClock) -> DotNetDateTime {
    empyrean_common::clock::Clock::utc_now(clock)
}
