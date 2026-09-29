//! ACE: Source/ACE.Server/Managers/WorldManager.cs::UpdateWorld
//! Shard callbacks run in their own loop stage before the world action queue, in completion
//! order, surviving a panicking one.
//! Fixture: synthetic dats, isolated world state.

use std::sync::{Arc, Mutex};

use empyrean_common::clock::{ClockSnapshot, VirtualClock};
use empyrean_dat::FakeDats;
use empyrean_world::entity::actions::i_action::Action;
use empyrean_world::entity::actions::i_actor::{enqueue, Actor};
use empyrean_world::entity::timers::TimersState;
use empyrean_world::managers::world_manager::{self as wm, NoWire};
use empyrean_world::World;

type Log = Arc<Mutex<Vec<String>>>;

fn push(l: &Log, s: impl Into<String>) {
    l.lock().unwrap().push(s.into());
}

fn entries(l: &Log) -> Vec<String> {
    l.lock().unwrap().clone()
}

fn world_on(clock: &VirtualClock) -> World {
    let timers = TimersState::new(clock);
    let now = ClockSnapshot::take(clock, timers.portal_year_ticks);
    let mut w = World::new(now, FakeDats::new().build().expect("empty fake dats"));
    w.timers = timers;
    w
}

fn tick(w: &mut World, clock: &VirtualClock) {
    let now = ClockSnapshot::take(clock, w.timers.portal_year_ticks);
    w.tick(now, &mut NoWire);
}

fn note(l: &Log, s: &str) -> Action {
    let (l, s) = (Arc::clone(l), s.to_owned());
    Action::delegate(move |_w: &mut World| push(&l, s))
}

#[test]
fn a_save_issued_by_a_world_action_calls_back_before_the_next_world_queue() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    let l: Log = Arc::default();

    let save_log = Arc::clone(&l);
    enqueue(
        &mut w,
        Actor::World,
        Action::delegate(move |w: &mut World| {
            push(&save_log, "save issued");
            let biota = empyrean_entity::Biota {
                id: 0x8000_0042,
                weenie_class_id: 1,
                ..Default::default()
            };
            let cb_log = Arc::clone(&save_log);
            w.shard.save_biota(
                biota,
                Some(Box::new(move |w: &mut World, ok: bool| {
                    push(
                        &cb_log,
                        format!("callback {ok} on {:?}", std::thread::current().id()),
                    );
                    // ACE's pattern: queue the processing of the result on the world queue.
                    let processed = Arc::clone(&cb_log);
                    wm::enqueue_action(
                        w,
                        Action::delegate(move |_w: &mut World| push(&processed, "processed")),
                    );
                })),
            );
        }),
    );

    tick(&mut w, &clock);
    assert_eq!(
        entries(&l),
        ["save issued"],
        "the callback waits for the callback stage"
    );
    assert!(
        w.shard
            .base_database()
            .get_biota(0x8000_0042, false)
            .is_some(),
        "the save itself completed"
    );

    // Next iteration: inbound queue, then the callback stage, then the world queue.
    enqueue(&mut w, Actor::World, note(&l, "world"));
    enqueue(&mut w, Actor::InboundMessageQueue, note(&l, "inbound"));
    tick(&mut w, &clock);
    let this_thread = format!("callback true on {:?}", std::thread::current().id());
    assert_eq!(
        entries(&l),
        ["save issued", "inbound", this_thread.as_str(), "world", "processed"],
        "the callback runs on the world thread after the inbound queue and before the world queue, \
         so what it queues is processed in the same iteration"
    );

    tick(&mut w, &clock);
    assert_eq!(entries(&l).len(), 5, "each callback runs once");
    assert_eq!(w.world_manager.stage_exceptions, 0);
}

#[test]
fn callbacks_run_in_completion_order_and_a_panicking_one_does_not_stop_the_rest() {
    let clock = VirtualClock::default();
    let mut w = world_on(&clock);
    let l: Log = Arc::default();

    let (a, c) = (Arc::clone(&l), Arc::clone(&l));
    w.shard.get_max_guid_found_in_range(
        0x8000_0000,
        0xFFFF_FFFE,
        Some(Box::new(move |_w: &mut World, v: u32| {
            push(&a, format!("max {v:08X}"))
        })),
    );
    w.shard.remove_biota(
        7,
        Some(Box::new(|_w: &mut World, _ok: bool| {
            panic!("callback threw")
        })),
    );
    w.shard.save_biota(
        empyrean_entity::Biota {
            id: 0x8000_0001,
            weenie_class_id: 1,
            ..Default::default()
        },
        Some(Box::new(move |_w: &mut World, ok: bool| {
            push(&c, format!("saved {ok}"))
        })),
    );

    wm::run_shard_callbacks(&mut w);
    assert_eq!(entries(&l), ["max FFFFFFFF", "saved true"]);
    assert_eq!(w.world_manager.shard_callback_exceptions, 1);
    assert_eq!(
        w.world_manager.stage_exceptions, 0,
        "caught per callback, not by the stage guard"
    );
    wm::run_shard_callbacks(&mut w);
    assert_eq!(entries(&l).len(), 2, "nothing left");
}
