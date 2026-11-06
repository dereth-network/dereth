//! Behaviors played on a made-up table and object.
//! Fixture: one element and mode; an idle on layer 0, an action and a second action on layer 1,
//! each animation one cycle a second; the action ramps over a quarter second.

use auberean_animation::host::{error, Event, Host};
use auberean_animation::queries::behavior_flags;
use auberean_animation::track::Params;
use auberean_animation::tracker::{Behaviors, IDLE};
use auberean_animation::triggers::{FrameTriggers, Gathered};
use auberean_assets::animation::{BehaviorKey, BehaviorStart, BehaviorTable};
use auberean_world::chargen::Random;

const E: u32 = 0x4000_0001;
const M: u32 = 0x4000_0001;
const ACTION: u32 = 0x4000_0100;
const OTHER: u32 = 0x4000_0101;

struct Object {
    events: Vec<Event>,
    triggers: FrameTriggers,
    dice: Random,
}

impl Host for Object {
    fn combo_cycle_velocity(&mut self, _key: &BehaviorKey) -> f32 {
        1.0
    }
    fn hold_loop(&mut self, _key: &BehaviorKey) -> Option<(f32, f32)> {
        None
    }
    fn behavior_flags(&self, behavior: u32) -> Option<u64> {
        match behavior {
            ACTION | OTHER => Some(behavior_flags::ACTION),
            IDLE => Some(0),
            _ => None,
        }
    }
    fn is_the_player(&self) -> bool {
        false
    }
    fn triggers(&mut self) -> Option<&mut FrameTriggers> {
        Some(&mut self.triggers)
    }
    fn script_triggers(&mut self, _script: u32, _time: f32, _g: &mut Gathered) {}
    fn dice(&mut self) -> &mut Random {
        &mut self.dice
    }
    fn dice_and_triggers(&mut self) -> (&mut Random, Option<&mut FrameTriggers>) {
        (&mut self.dice, Some(&mut self.triggers))
    }
    fn event(&mut self, e: Event) {
        self.events.push(e);
    }
}

fn table() -> BehaviorTable {
    let combo = |behavior, layer| {
        (
            BehaviorKey {
                element: E,
                mode: M,
                behavior,
            },
            (behavior, layer),
        )
    };
    BehaviorTable {
        combos: vec![combo(IDLE, 0), combo(ACTION, 1), combo(OTHER, 1)],
        layers: vec![0, 1],
        behaviors: vec![(
            ACTION,
            BehaviorStart {
                ramp_time: 0.25,
                effect: 0,
            },
        )],
        initial_behaviors: vec![IDLE],
        initial_mode: M,
        ..BehaviorTable::default()
    }
}

fn object() -> Object {
    Object {
        events: Vec::new(),
        triggers: FrameTriggers::default(),
        dice: Random::default(),
    }
}

/// An action ramps its weight in over its ramp time, plays its cycle once, ramps out over the
/// last ramp time of its cycle and is dropped; whoever waits on it hears it ended. While a
/// behavior the server asked for plays, the idles stop; when it ends, they start again.
///
/// Behaviour: animation.behaviors.an-action-ramps-in-plays-once-and-ends
#[test]
fn an_action_ramps_in_plays_once_and_ends() {
    let t = table();
    let mut o = object();
    let mut tr = Behaviors::new();
    tr.init(&t, E, M, None, &mut o);
    assert!(tr.is_doing_behavior(IDLE), "the initial behavior plays");
    let p = Params {
        context: 9,
        ..Params::behavior(ACTION)
    };
    assert!(tr.do_behavior(&t, &p, &mut o));
    assert!(tr.stop_idle, "a server behavior stops the idles");
    let mut weights = Vec::new();
    for _ in 0..12 {
        tr.update(&t, 0.1, &mut o);
        weights.push(tr.track(ACTION).map(|t| t.weight));
    }
    let w = |k: usize| weights[k].expect("still playing");
    assert!((w(0) - 0.4).abs() < 1e-4, "ramping in: {weights:?}");
    assert!((w(4) - 1.0).abs() < 1e-6, "full in the middle: {weights:?}");
    assert!(w(8) < 0.5, "ramping out: {weights:?}");
    assert!(weights[11].is_none(), "dropped once its weight is spent");
    assert!(o.events.contains(&Event::BehaviorDone {
        context: 9,
        error: error::NONE
    }));
    assert!(tr.start_idle, "the idles start again");
}

/// What an object does of itself is not told to the network, so a description that does not list
/// it leaves it alone; what it was asked to do is.
///
/// Behaviour: animation.behaviors.what-an-object-does-of-itself-is-not-the-networks
#[test]
fn a_behavior_an_object_does_of_itself_is_not_among_those_told_to_the_network() {
    let t = table();
    let mut o = object();
    let mut tr = Behaviors::new();
    tr.init(&t, E, M, None, &mut o);
    assert!(tr.do_autonomous_behavior(&t, ACTION, &mut o));
    tr.update(&t, 0.1, &mut o);
    assert!(tr.track(ACTION).is_some_and(|t| t.weight > 0.0), "it plays");
    assert!(
        tr.network_params().iter().all(|p| p.behavior != ACTION),
        "done of itself"
    );
    assert!(tr.do_behavior(&t, &Params::behavior(OTHER), &mut o));
    tr.update(&t, 0.1, &mut o);
    assert!(
        tr.network_params().iter().any(|p| p.behavior == OTHER),
        "asked for"
    );
}

/// Playing a behavior on a layer another holds stops the holder (whoever waits on it hears it was
/// stopped) and takes the layer; a behavior already playing is refused.
///
/// Behaviour: animation.behaviors.a-new-behavior-takes-its-layer
#[test]
fn a_new_behavior_takes_its_layer() {
    let t = table();
    let mut o = object();
    let mut tr = Behaviors::new();
    tr.init(&t, E, M, None, &mut o);
    let first = Params {
        context: 1,
        ..Params::behavior(ACTION)
    };
    assert!(tr.do_behavior(&t, &first, &mut o));
    assert!(!tr.do_behavior(&t, &first, &mut o), "already playing");
    assert!(o.events.contains(&Event::BehaviorDone {
        context: 1,
        error: error::ALREADY_PLAYING
    }));
    assert!(tr.do_behavior(&t, &Params::behavior(OTHER), &mut o));
    assert_eq!(tr.active_by_layer.get(1), Some(&OTHER));
    assert!(tr.track(ACTION).is_some_and(|t| t.ramping_down));
    assert!(o.events.contains(&Event::BehaviorDone {
        context: 1,
        error: error::STOPPED
    }));
}
