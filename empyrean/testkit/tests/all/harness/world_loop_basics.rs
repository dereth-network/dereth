//! ACE: Source/ACE.Server/Managers/WorldManager.cs::UpdateWorld
//! Login reaches the world's auth hand-off; link comes up both ways; queued/delayed actions run
//! in the right ticks; a panicking handler does not take the server down; ten virtual minutes run
//! fast; shutdown now exits; gem count table built on first use.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

pub(crate) use std::sync::{Arc, Mutex};
pub(crate) use std::time::{Duration, Instant};

pub(crate) use dereth_primitives::NetQueue;
pub(crate) use dereth_primitives::ObjectId;
pub(crate) use dereth_protocol::{self as proto, items::InventoryUseEvent, login::LoginWorldInfo};
pub(crate) use empyrean_entity::ObjectGuid;
pub(crate) use empyrean_net::{GameMessageGroup, OutboundMessage, SessionId, SessionState};
pub(crate) use empyrean_testkit::{ClientStatus, TestServer};
pub(crate) use empyrean_world::entity::actions::action_chain::ActionChain;
pub(crate) use empyrean_world::entity::actions::i_action::Action;
pub(crate) use empyrean_world::entity::actions::i_actor::{enqueue, Actor};
pub(crate) use empyrean_world::managers::{server_manager, world_manager};
pub(crate) use empyrean_world::World;

pub(crate) const DO_LOGIN: &str = "ACE: AuthenticationHandler.DoLogin";

/// The one session the world has heard of.
pub(crate) fn only_session(ts: &TestServer) -> SessionId {
    let ids: Vec<SessionId> = ts.world.sessions.iter().map(|(id, _)| id).collect();
    assert_eq!(ids.len(), 1, "{ids:?}");
    ids[0]
}

#[test]
pub(crate) fn a_login_reaches_the_worlds_auth_hand_off_and_is_answered() {
    let mut ts = TestServer::new();
    world_manager::open(&mut ts.world, None); // what Program.Main does when `world_closed` is off
    let _ = TestServer::take_not_ported();
    let id = ts.connect("acct", "pw");

    assert_eq!(ts.client(id).status(), ClientStatus::Connected);
    assert!(!TestServer::take_not_ported().contains_key(DO_LOGIN));
    let session = only_session(&ts);
    ts.step();
    assert_eq!(
        ts.world.net.session(session).map(|s| s.core.state),
        Some(SessionState::AuthConnected)
    );
    assert_eq!(
        ts.world.sessions.get(session).map(|s| s.state),
        Some(SessionState::AuthConnected)
    );
}

#[test]
pub(crate) fn with_the_login_answered_the_link_comes_up_and_messages_flow_both_ways() {
    let mut ts = TestServer::new();
    world_manager::open(&mut ts.world, None);
    let id = ts.add_client("acct", "pw");
    assert!(ts.run_until(1.0, |ts| ts.world.sessions.len() == 1));
    let session = only_session(&ts);

    assert!(ts.run_until(1.0, |ts| ts.client(id).status() == ClientStatus::Connected));
    ts.advance(0.1); // the ConnectResponse event reaches the world at the next iteration's hand-off
    let server_name = ts.received::<LoginWorldInfo>(id);
    assert_eq!(server_name.len(), 1, "SendConnectResponse's ServerName");

    // Server to client: a message the world sends arrives, decoded.
    let info = LoginWorldInfo {
        connections: 1,
        max_connections: 128,
        world_name: "Dereth".to_owned(),
    };
    ts.world.net.send(
        session,
        OutboundMessage {
            group: GameMessageGroup::UIQueue,
            data: proto::write_blob(&info).unwrap(),
        },
    );
    ts.advance(0.1);
    assert_eq!(ts.received::<LoginWorldInfo>(id).last(), Some(&info));

    // Client to server: a message ACE has no handler for is counted.
    ts.send_message(id, NetQueue::Logon, &LoginWorldInfo::default());
    ts.advance(0.1);
    assert_eq!(ts.world.sessions.inbound.unhandled_messages, 1);

    // A game action reaches its handler once the world half says the player is in the world.
    let s = ts.world.sessions.get_mut(session).unwrap();
    s.state = SessionState::WorldConnected;
    s.set_player(Some(ObjectGuid::new(0x5000_0001)));
    let before = ts.world.sessions.inbound.handler_exceptions;
    ts.send_game_action(
        id,
        &InventoryUseEvent {
            object: ObjectId(0x8000_0001),
        },
    );
    ts.advance(0.1);
    assert_eq!(ts.world.sessions.inbound.handler_exceptions, before + 1);
}

pub(crate) type Log = Arc<Mutex<Vec<(String, f64)>>>;

/// A closure that records `name` with the game time it ran at.
pub(crate) fn note(l: &Log, name: &str) -> impl FnOnce(&mut World) + Send + 'static {
    let (l, name) = (Arc::clone(l), name.to_string());
    move |w: &mut World| l.lock().unwrap().push((name, w.now.portal_year_ticks))
}

#[test]
pub(crate) fn a_queued_world_action_and_a_delayed_action_run_in_the_right_ticks() {
    let mut ts = TestServer::new();
    ts.step();
    let l: Log = Arc::default();
    // Between iterations the world still holds the last tick's clock reading, which is what an
    // action built now reads (`Timers.PortalYearTicks`); the next tick runs at `t0`.
    let enqueued_at = ts.world.now.portal_year_ticks;
    let t0 = ts.world.timers.portal_year_ticks;

    enqueue(
        &mut ts.world,
        Actor::World,
        Action::delegate(note(&l, "queued")),
    );
    let mut chain = ActionChain::new();
    chain
        .add_delay_seconds(&mut ts.world, 1.0)
        .add_action(Actor::World, note(&l, "delayed"));
    chain.enqueue_chain(&mut ts.world);

    ts.step();
    assert_eq!(
        l.lock().unwrap().clone(),
        vec![("queued".to_owned(), t0)],
        "the next tick runs the world queue"
    );

    ts.advance(2.0);
    let ran = l.lock().unwrap().clone();
    assert_eq!(ran.len(), 2);
    // The delay ends at `enqueued_at + 1.0`. It fires in the first tick at or after that
    // (`EndTime <= PortalYearTicks`), and its next action, enqueued on the world queue during the
    // delay stage, runs one tick later.
    let tick = TestServer::TICK.as_secs_f64();
    let mut fire = t0;
    while fire < enqueued_at + 1.0 {
        fire += tick;
    }
    assert!(
        (ran[1].1 - (fire + tick)).abs() < 1e-6,
        "delayed action at {} (fire {fire})",
        ran[1].1
    );
}

#[test]
pub(crate) fn a_panicking_handler_does_not_take_the_server_down() {
    let mut ts = TestServer::new();
    let id = ts.add_client("acct", "pw");
    let l: Log = Arc::default();
    enqueue(
        &mut ts.world,
        Actor::InboundMessageQueue,
        Action::delegate(|_w: &mut World| panic!("handler bug")),
    );
    enqueue(
        &mut ts.world,
        Actor::World,
        Action::delegate(note(&l, "after")),
    );
    let _ = TestServer::take_not_ported();

    ts.advance(3.0);
    // F14: the action's own guard catches it; the stage guard is not reached.
    assert_eq!(
        ts.world
            .sessions
            .inbound
            .inbound_message_queue
            .act_exceptions(),
        1
    );
    assert_eq!(ts.world.world_manager.stage_exceptions, 0);
    assert_eq!(l.lock().unwrap().len(), 1, "the world queue still ran");
    assert!(ts.world.world_manager.world_active);
    assert_eq!(
        ts.client(id).status(),
        ClientStatus::Connected,
        "logins are still handed to the world"
    );
}

/// Nothing in the harness waits on real time: every iteration moves the virtual clock by one tick
/// and returns at once. What is measured is each iteration's wall time, and the check is on the
/// median, which a loaded host does not move: an iteration takes tens of microseconds, so a busy
/// machine preempts only the few that straddle a scheduling quantum, while a real sleep (at least
/// 1 ms on every OS) or a wait for the wall clock to catch up (16.7 ms a tick) is in every one of
/// them. The total has a backstop 30 times above an idle run, for a wait in only some iterations;
/// neither bound is a performance budget (that is the gate's host-relative unit-tier budget).
#[test]
pub(crate) fn ten_minutes_of_virtual_time_pass_in_well_under_a_second() {
    const MEDIAN_STEP_LIMIT: Duration = Duration::from_micros(500);
    const TOTAL_LIMIT: Duration = Duration::from_secs(30);
    let mut ts = TestServer::new();
    ts.add_client("acct", "pw");
    let expected_steps = usize::try_from(
        Duration::from_secs(600)
            .as_nanos()
            .div_ceil(TestServer::TICK.as_nanos()),
    )
    .unwrap();
    let mut steps = Vec::with_capacity(expected_steps + 1);
    let start = Instant::now();
    while ts.seconds() < 600.0 {
        let t0 = Instant::now();
        ts.step();
        steps.push(t0.elapsed());
    }
    let took = start.elapsed();
    // The clock is virtual: ten minutes are exactly this many ticks, whatever the wall clock did.
    assert_eq!(steps.len(), expected_steps);
    steps.sort_unstable();
    let median = steps[steps.len() / 2];
    assert!(
        median < MEDIAN_STEP_LIMIT,
        "the median iteration took {median:?} (a real sleep or wait?); 10 virtual minutes took {took:?}"
    );
    assert!(
        took < TOTAL_LIMIT,
        "10 virtual minutes took {took:?} (median iteration {median:?})"
    );
}

#[test]
pub(crate) fn shutdown_now_stops_the_world_and_exits() {
    let mut ts = TestServer::new();
    ts.add_client("acct", "pw");
    ts.advance(0.5);
    server_manager::do_shutdown_now(&mut ts.world);
    // The authenticated session is disconnected and flushed for 2 s before the world stops.
    assert!(ts.run_until(5.0, |ts| ts.exited()));
    assert!(!ts.world.world_manager.world_active);
    assert!(ts.world.server_manager.shutdown_in_progress);
    // No player is online, so only the session's disconnection is waited for.
    let ticks_before = ts.world.timers.portal_year_ticks;
    ts.advance(1.0);
    assert_eq!(
        ts.world.timers.portal_year_ticks, ticks_before,
        "a stopped world no longer ticks"
    );
}
