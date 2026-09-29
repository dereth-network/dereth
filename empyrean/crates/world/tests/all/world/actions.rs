//! ACE: Source/ACE.Server/Entity/Actions/ActionQueue.cs::RunActions
//! Action chains, world-queue count snapshot, delay manager (EndTime, sequence) order, branches,
//! loops, detached routing and Timers follow ACE.
//! Fixture: synthetic dats, isolated world state.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_common::dereth_date_time::DerethDateTime;
use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_dat::FakeDats;
use empyrean_entity::{LandblockId, ObjectGuid};
use empyrean_world::entity::actions::action_chain::{ActionChain, ChainElement};
use empyrean_world::entity::actions::action_queue::run_actions;
use empyrean_world::entity::actions::conditional_action::ConditionalAction;
use empyrean_world::entity::actions::delay_action::DelayAction;
use empyrean_world::entity::actions::delay_manager;
use empyrean_world::entity::actions::i_action::Action;
use empyrean_world::entity::actions::i_actor::{enqueue, Actor};
use empyrean_world::entity::timers::{self, TimersState};
use empyrean_world::World;

const T0: f64 = 1000.0;

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: T0,
        unix_time: 0.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(now, FakeDats::new().build().expect("empty fake dats"))
}

type Log = Arc<Mutex<Vec<String>>>;

fn log() -> Log {
    Arc::default()
}

fn entries(l: &Log) -> Vec<String> {
    l.lock().unwrap().clone()
}

/// A closure that appends `name` to `l`.
fn note(l: &Log, name: &str) -> impl FnOnce(&mut World) + Send + 'static {
    let (l, name) = (Arc::clone(l), name.to_string());
    move |_w: &mut World| l.lock().unwrap().push(name)
}

fn run_world(w: &mut World) {
    run_actions(w, Actor::World);
}

fn world_len(w: &World) -> usize {
    w.world_manager.action_queue.len()
}

#[test]
fn chain_runs_in_order_across_actors() {
    let (mut w, l) = (world(), log());
    let mut chain = ActionChain::new();
    chain.add_action(Actor::World, note(&l, "a"));
    chain.add_delay_seconds(&mut w, 1.0);
    chain.add_action(Actor::World, note(&l, "b"));
    chain.add_action(Actor::World, note(&l, "c"));
    chain.enqueue_chain(&mut w);
    assert_eq!(world_len(&w), 1, "only the first element is enqueued");

    run_world(&mut w);
    assert_eq!(entries(&l), ["a"]);
    assert_eq!(
        w.world_manager.delay_manager.len(),
        1,
        "a's NextAct went to the delay manager"
    );
    assert_eq!(world_len(&w), 0);

    delay_manager::run_actions(&mut w);
    assert_eq!(
        w.world_manager.delay_manager.len(),
        1,
        "not due before T0 + 1"
    );

    w.now.portal_year_ticks = T0 + 1.0;
    delay_manager::run_actions(&mut w);
    assert!(
        w.world_manager.delay_manager.is_empty(),
        "EndTime <= now runs"
    );
    assert_eq!(
        entries(&l),
        ["a"],
        "the delay's NextAct is enqueued, not run"
    );
    run_world(&mut w);
    assert_eq!(
        entries(&l),
        ["a", "b"],
        "b's continuation waits for the next run"
    );
    run_world(&mut w);
    assert_eq!(entries(&l), ["a", "b", "c"]);
}

#[test]
fn actions_enqueued_during_a_run_wait_for_the_next_run() {
    let (mut w, l) = (world(), log());
    let inner = note(&l, "enqueued-during-run");
    let l2 = Arc::clone(&l);
    enqueue(
        &mut w,
        Actor::World,
        Action::delegate(move |w: &mut World| {
            l2.lock().unwrap().push("first".into());
            enqueue(w, Actor::World, Action::delegate(inner));
        }),
    );
    enqueue(&mut w, Actor::World, Action::delegate(note(&l, "second")));

    run_world(&mut w);
    assert_eq!(
        entries(&l),
        ["first", "second"],
        "count snapshot: 2 at entry, 2 run"
    );
    assert_eq!(world_len(&w), 1, "the new action is in the live queue");
    run_world(&mut w);
    assert_eq!(entries(&l), ["first", "second", "enqueued-during-run"]);
    assert_eq!(world_len(&w), 0);
}

#[test]
fn clear_during_a_run_drops_the_waiting_actions() {
    let (mut w, l) = (world(), log());
    let l2 = Arc::clone(&l);
    enqueue(
        &mut w,
        Actor::World,
        Action::delegate(move |w: &mut World| {
            l2.lock().unwrap().push("clearer".into());
            w.world_manager.action_queue.clear();
        }),
    );
    enqueue(&mut w, Actor::World, Action::delegate(note(&l, "cleared")));
    run_world(&mut w);
    run_world(&mut w);
    assert_eq!(
        entries(&l),
        ["clearer"],
        "TryDequeue fails for the cleared action"
    );
}

#[test]
fn delays_order_by_end_time_then_build_sequence() {
    let (mut w, l) = (world(), log());
    // Built in order x, y, z: sequences 1, 2, 3.
    let mut x = ActionChain::new();
    x.add_delay_seconds(&mut w, 1.0)
        .add_action(Actor::World, note(&l, "x"));
    let mut y = ActionChain::new();
    y.add_delay_seconds(&mut w, 1.0)
        .add_action(Actor::World, note(&l, "y"));
    let mut z = ActionChain::new();
    z.add_delay_seconds(&mut w, 0.5)
        .add_action(Actor::World, note(&l, "z"));
    assert_eq!(w.world_manager.delay_manager.glbl_sequence(), 3);

    // Enqueued in reverse: y, x tie on EndTime and break by sequence (build order), not by enqueue.
    y.enqueue_chain(&mut w);
    x.enqueue_chain(&mut w);
    z.enqueue_chain(&mut w);
    let min = w.world_manager.delay_manager.min().expect("three delays");
    assert_eq!((min.end_time(), min.sequence()), (T0 + 0.5, 3));

    w.now.portal_year_ticks = T0 + 1.0;
    delay_manager::run_actions(&mut w);
    run_world(&mut w);
    assert_eq!(entries(&l), ["z", "x", "y"]);
}

#[test]
fn end_time_is_set_at_enqueue_not_at_build() {
    let (mut w, l) = (world(), log());
    let mut chain = ActionChain::new();
    chain
        .add_delay_seconds(&mut w, 5.0)
        .add_action(Actor::World, note(&l, "late"));

    w.now.portal_year_ticks = T0 + 100.0;
    chain.enqueue_chain(&mut w);
    assert_eq!(
        w.world_manager
            .delay_manager
            .min()
            .map(DelayAction::end_time),
        Some(T0 + 105.0)
    );

    w.now.portal_year_ticks = T0 + 104.5;
    delay_manager::run_actions(&mut w);
    assert_eq!(w.world_manager.delay_manager.len(), 1);
    w.now.portal_year_ticks = T0 + 105.0;
    delay_manager::run_actions(&mut w);
    run_world(&mut w);
    assert_eq!(entries(&l), ["late"]);
}

#[test]
fn add_delay_seconds_zero_negative_and_nan_add_nothing() {
    let mut w = world();
    let mut chain = ActionChain::new();
    chain.add_delay_seconds(&mut w, 0.0);
    chain.add_delay_seconds(&mut w, -1.0);
    chain.add_delay_seconds(&mut w, f64::NAN);
    assert!(chain.is_empty());
    assert_eq!(
        w.world_manager.delay_manager.glbl_sequence(),
        0,
        "no DelayAction was constructed"
    );

    chain.add_delay_seconds(&mut w, 2.0);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain.first_element().map(|e| e.actor), Some(Actor::Delay));
    assert!(
        matches!(&chain.first_element().unwrap().action, Action::Delay(d) if d.wait_time() == 2.0 && d.sequence() == 1)
    );
}

#[test]
fn add_delay_for_one_tick_waits_0_001_float() {
    let mut w = world();
    let mut chain = ActionChain::new();
    chain.add_delay_for_one_tick(&mut w);
    let Action::Delay(d) = &chain.first_element().expect("one element").action else {
        panic!("a DelayAction")
    };
    // ACE writes 0.001f: the float's value widened to double, not 0.001.
    assert_eq!(d.wait_time(), 0.0010000000474974513);
    assert_ne!(d.wait_time(), 0.001);

    chain.enqueue_chain(&mut w);
    let end = w
        .world_manager
        .delay_manager
        .min()
        .map(DelayAction::end_time);
    assert_eq!(end, Some(T0 + 0.0010000000474974513));
}

fn branch_run(cond: bool) -> Vec<String> {
    let (mut w, l) = (world(), log());
    let mut t = ActionChain::new();
    t.add_action(Actor::World, note(&l, "true-1"))
        .add_action(Actor::World, note(&l, "true-2"));
    let f = ActionChain::with_action(Actor::World, note(&l, "false-1"));
    let lc = Arc::clone(&l);
    let mut chain = ActionChain::with_action(Actor::World, note(&l, "before"));
    chain
        .add_branch(
            Actor::World,
            move |_w: &mut World| {
                lc.lock().unwrap().push("cond".into());
                cond
            },
            t,
            f,
        )
        .add_action(Actor::World, note(&l, "after"));
    chain.enqueue_chain(&mut w);
    for _ in 0..10 {
        run_world(&mut w);
    }
    entries(&l)
}

#[test]
fn conditional_takes_each_branch_then_continues() {
    assert_eq!(
        branch_run(true),
        ["before", "cond", "true-1", "true-2", "after"]
    );
    assert_eq!(branch_run(false), ["before", "cond", "false-1", "after"]);
}

#[test]
fn conditional_branch_elements_overload() {
    let (mut w, l) = (world(), log());
    let mut chain = ActionChain::new();
    chain.add_branch_elements(
        Actor::World,
        |_w: &mut World| false,
        ChainElement::new(Actor::World, Action::delegate(note(&l, "t"))),
        ChainElement::new(Actor::World, Action::delegate(note(&l, "f"))),
    );
    chain.enqueue_chain(&mut w);
    run_world(&mut w);
    run_world(&mut w);
    assert_eq!(entries(&l), ["f"]);
}

#[test]
#[should_panic(expected = "NullReferenceException")]
fn conditional_on_an_empty_branch_with_nothing_after_throws() {
    let mut w = world();
    let action = ConditionalAction::new(
        Box::new(|_w: &mut World| true),
        ActionChain::new(),
        ActionChain::new(),
    );
    let _ = Action::Conditional(action).act(&mut w);
}

#[test]
fn conditional_on_an_empty_branch_throws_inside_the_queue_and_is_caught() {
    let (mut w, l) = (world(), log());
    let mut chain = ActionChain::new();
    chain.add_branch(
        Actor::World,
        |_w: &mut World| true,
        ActionChain::new(),
        ActionChain::new(),
    );
    chain.enqueue_chain(&mut w);
    enqueue(&mut w, Actor::World, Action::delegate(note(&l, "after")));
    run_world(&mut w);
    assert_eq!(w.world_manager.action_queue.act_exceptions(), 1);
    assert_eq!(entries(&l), ["after"]);
}

#[test]
fn loop_repeats_its_body_until_the_condition_is_false() {
    let (mut w, l) = (world(), log());
    let count = Arc::new(Mutex::new(0));
    let (lc, cc) = (Arc::clone(&l), Arc::clone(&count));
    let (lb, cb) = (Arc::clone(&l), Arc::clone(&count));
    let mut chain = ActionChain::new();
    chain
        .add_loop(
            Actor::World,
            move |_w: &mut World| {
                let n = *cc.lock().unwrap();
                lc.lock().unwrap().push(format!("cond{n}"));
                n < 3
            },
            move |_w: &mut World| {
                let (lb, cb) = (Arc::clone(&lb), Arc::clone(&cb));
                ActionChain::with_action(Actor::World, move |_w: &mut World| {
                    let mut n = cb.lock().unwrap();
                    *n += 1;
                    lb.lock().unwrap().push(format!("body{n}"));
                })
            },
        )
        .add_action(Actor::World, note(&l, "after"));
    chain.enqueue_chain(&mut w);

    run_world(&mut w);
    assert_eq!(
        entries(&l),
        ["cond0"],
        "one step per run: the body waits for the next run"
    );
    for _ in 0..10 {
        run_world(&mut w);
    }
    assert_eq!(
        entries(&l),
        ["cond0", "body1", "cond1", "body2", "cond2", "body3", "cond3", "after"]
    );
    assert_eq!(world_len(&w), 0);
}

#[test]
fn delays_due_at_entry_all_run_in_one_call() {
    let (mut w, l) = (world(), log());
    for (name, wait) in [("p", 3.0), ("q", 1.0), ("r", 2.0)] {
        let mut c = ActionChain::new();
        c.add_delay_seconds(&mut w, wait)
            .add_action(Actor::World, note(&l, name));
        c.enqueue_chain(&mut w);
    }
    w.now.portal_year_ticks = T0 + 10.0;
    delay_manager::run_actions(&mut w);
    assert!(w.world_manager.delay_manager.is_empty());
    run_world(&mut w);
    assert_eq!(entries(&l), ["q", "r", "p"]);
}

#[test]
fn a_delay_made_due_during_a_pass_runs_on_the_next_call() {
    // ACE's `checkNeeded` is never set back to true: one pass per call. A delay that a due delay's
    // NextAct puts on the manager, already due (a zero wait), waits for the next call.
    let (mut w, l) = (world(), log());
    let mut chain = ActionChain::new();
    chain.add_delay_seconds(&mut w, 1.0);
    let zero = DelayAction::new(&mut w, 0.0);
    chain.add_iaction(Actor::Delay, Action::Delay(zero));
    chain.add_action(Actor::World, note(&l, "x"));
    chain.enqueue_chain(&mut w);

    w.now.portal_year_ticks = T0 + 1.0;
    delay_manager::run_actions(&mut w);
    assert_eq!(
        w.world_manager.delay_manager.len(),
        1,
        "the zero-wait delay is held, not run"
    );
    assert_eq!(
        w.world_manager
            .delay_manager
            .min()
            .map(DelayAction::end_time),
        Some(T0 + 1.0)
    );
    assert_eq!(world_len(&w), 0);

    delay_manager::run_actions(&mut w);
    assert!(
        w.world_manager.delay_manager.is_empty(),
        "due at the same time, it runs next call"
    );
    run_world(&mut w);
    assert_eq!(entries(&l), ["x"]);
}

#[test]
fn a_following_delay_starts_when_the_first_one_fires() {
    let (mut w, l) = (world(), log());
    let mut chain = ActionChain::new();
    chain
        .add_delay_seconds(&mut w, 1.0)
        .add_delay_seconds(&mut w, 1.0);
    chain.add_action(Actor::World, note(&l, "done"));
    chain.enqueue_chain(&mut w);
    w.now.portal_year_ticks = T0 + 1.5;
    delay_manager::run_actions(&mut w);
    assert_eq!(
        w.world_manager
            .delay_manager
            .min()
            .map(DelayAction::end_time),
        Some(T0 + 2.5)
    );
    w.now.portal_year_ticks = T0 + 2.5;
    delay_manager::run_actions(&mut w);
    run_world(&mut w);
    assert_eq!(entries(&l), ["done"]);
}

#[test]
fn delay_manager_drops_a_non_delay_action() {
    let (mut w, l) = (world(), log());
    enqueue(&mut w, Actor::Delay, Action::delegate(note(&l, "never")));
    assert!(w.world_manager.delay_manager.is_empty());
    w.now.portal_year_ticks = T0 + 100.0;
    delay_manager::run_actions(&mut w);
    run_world(&mut w);
    assert!(entries(&l).is_empty());
}

#[test]
fn routing_to_a_missing_object_or_unloaded_landblock_drops_the_action() {
    let (mut w, l) = (world(), log());
    empyrean_common::not_ported::take_local();

    enqueue(
        &mut w,
        Actor::Object(ObjectGuid::new(0x8000_0001)),
        Action::delegate(note(&l, "obj")),
    );
    // A landblock that is not loaded never runs its queue again.
    enqueue(
        &mut w,
        Actor::Landblock(LandblockId::new(0xA9B4_FFFF)),
        Action::delegate(note(&l, "lb")),
    );
    assert!(empyrean_common::not_ported::take_local().is_empty());

    // A chain whose continuation targets a missing object: the continuation is dropped when routed.
    let mut chain = ActionChain::with_action(Actor::World, note(&l, "world"));
    chain.add_action(
        Actor::Object(ObjectGuid::new(0x8000_0002)),
        note(&l, "obj2"),
    );
    chain.enqueue_chain(&mut w);
    run_world(&mut w);
    run_world(&mut w);
    assert!(empyrean_common::not_ported::take_local().is_empty());

    assert_eq!(entries(&l), ["world"]);
    assert_eq!(world_len(&w), 0);
    assert!(w.world_manager.delay_manager.is_empty());
}

#[test]
fn add_chain_appends_or_adopts() {
    let (mut w, l) = (world(), log());
    let mut empty = ActionChain::new();
    empty.add_chain(None).add_chain(Some(ActionChain::new()));
    assert!(empty.is_empty());
    empty.add_chain(Some(ActionChain::with_action(
        Actor::World,
        note(&l, "adopted"),
    )));
    let mut tail = ActionChain::with_action(Actor::World, note(&l, "tail"));
    tail.add_iaction(Actor::World, Action::Empty(Default::default()));
    tail.add_action(Actor::World, note(&l, "end"));
    empty.add_chain(Some(tail));
    assert_eq!(empty.len(), 4);
    empty.enqueue_chain(&mut w);
    for _ in 0..5 {
        run_world(&mut w);
    }
    assert_eq!(entries(&l), ["adopted", "tail", "end"]);
}

#[test]
fn world_manager_enqueue_action_goes_to_the_world_queue() {
    let (mut w, l) = (world(), log());
    empyrean_world::managers::world_manager::enqueue_action(
        &mut w,
        Action::delegate(note(&l, "w")),
    );
    assert_eq!(world_len(&w), 1);
    run_world(&mut w);
    assert_eq!(entries(&l), ["w"]);
}

#[test]
fn timers_read_the_tick_snapshot_and_advance_by_elapsed_time() {
    let clock = VirtualClock::new(DotNetDateTime::new(2026, 1, 1));
    let t = TimersState::new(&clock);
    let emu = DerethDateTime::utc_now_to_emu_time(&clock).ticks();
    assert_eq!((t.world_start_lore_ticks, t.portal_year_ticks), (emu, emu));
    assert_eq!(t.world_start_time, DotNetDateTime::new(2026, 1, 1));

    let mut w = world();
    w.timers = TimersState::from_snapshot(&w.now);
    assert_eq!(timers::portal_year_ticks(&w), T0);
    assert_eq!(timers::running_time(&w), 0.0);

    w.now.utc = w.now.utc.add_seconds(90.0);
    assert_eq!(timers::running_time(&w), 90.0);
    assert_eq!(
        timers::current_time(&w),
        DotNetDateTime::new(2026, 1, 1).add_seconds(90.0)
    );

    let next = timers::advance_portal_year_ticks(&mut w, TimeSpan::from_ticks(2_500_000));
    assert_eq!(next, T0 + 0.25);
    assert_eq!(
        timers::portal_year_ticks(&w),
        T0,
        "readers see the snapshot until the next tick"
    );
    assert_eq!(timers::current_in_game_time(&w).ticks(), T0);
    assert_eq!(timers::world_start_lore_time(&w).ticks(), T0);
    assert_eq!(
        timers::current_lore_time(&w),
        DerethDateTime::convert_real_world_to_lore_date_time(w.now.utc)
    );
}

// ---- F14: one panicking action does not take the rest of the pass with it ----------------------

/// Held by an action routed to a detached object: `enqueue` drops that action, and this panics
/// as it is dropped, which is a panic inside the delay manager's pass.
struct PanicOnDrop;

impl Drop for PanicOnDrop {
    fn drop(&mut self) {
        panic!("F14 test: dropped");
    }
}

#[test]
fn a_panicking_action_is_caught_and_the_rest_of_the_queue_runs() {
    let (mut w, l) = (world(), log());
    enqueue(&mut w, Actor::World, Action::delegate(note(&l, "before")));
    enqueue(
        &mut w,
        Actor::World,
        Action::delegate(|_w: &mut World| panic!("F14 test: action")),
    );
    enqueue(&mut w, Actor::World, Action::delegate(note(&l, "after")));

    run_world(&mut w);
    assert_eq!(
        entries(&l),
        ["before", "after"],
        "the action after the panic runs in the same pass"
    );
    assert_eq!(world_len(&w), 0);
    assert_eq!(w.world_manager.action_queue.act_exceptions(), 1);

    run_world(&mut w);
    assert_eq!(entries(&l), ["before", "after"], "nothing left over");
}

#[test]
fn a_panic_mid_delay_pass_loses_no_due_delay() {
    let (mut w, l) = (world(), log());
    let mut first = ActionChain::new();
    first
        .add_delay_seconds(&mut w, 1.0)
        .add_action(Actor::World, note(&l, "first"));
    first.enqueue_chain(&mut w);

    // Due second: its NextAct goes to a detached object and panics as it is dropped.
    let mut bad = ActionChain::new();
    let held = PanicOnDrop;
    bad.add_delay_seconds(&mut w, 2.0).add_action(
        Actor::Object(ObjectGuid::new(0x5000_0001)),
        move |_w: &mut World| {
            let _held = &held;
        },
    );
    bad.enqueue_chain(&mut w);

    for (name, wait) in [("third", 3.0), ("fourth", 4.0)] {
        let mut c = ActionChain::new();
        c.add_delay_seconds(&mut w, wait)
            .add_action(Actor::World, note(&l, name));
        c.enqueue_chain(&mut w);
    }
    let mut later = ActionChain::new();
    later
        .add_delay_seconds(&mut w, 60.0)
        .add_action(Actor::World, note(&l, "later"));
    later.enqueue_chain(&mut w);

    w.now.portal_year_ticks = T0 + 10.0;
    delay_manager::run_actions(&mut w);
    assert_eq!(w.world_manager.delay_manager.act_exceptions(), 1);
    assert_eq!(
        w.world_manager.delay_manager.len(),
        1,
        "only the delay not yet due is held"
    );
    run_world(&mut w);
    assert_eq!(
        entries(&l),
        ["first", "third", "fourth"],
        "the due delays after the panic still ran"
    );

    w.now.portal_year_ticks = T0 + 60.0;
    delay_manager::run_actions(&mut w);
    run_world(&mut w);
    assert_eq!(entries(&l), ["first", "third", "fourth", "later"]);
}
