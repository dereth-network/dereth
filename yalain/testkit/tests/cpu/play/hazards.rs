//! Hazards: placed objects or spots a world's rows make harm a player who uses one or comes within
//! its reach. Fixture: the test hall beside where a character starts, a made-up gate standing in
//! it, and a made-up effect that takes health.

use auberean_primitives::Vec3;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::hazards::{Harm, Hazard, HazardTable};
use yalain_content::{ContentObject, Description, Facts, Kind};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{Node, Value};
use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};

const USE: u32 = 0x8216_0016;
const HALL: u32 = 0x0299_0109;
const GATE: u64 = 0x4002_9900_0000_0E91;
const GATE_T: u32 = 0x4700_2EF5;
const GATE_AT: [f32; 3] = [58.0, 241.0, -44.6];
/// A made-up effect taking 10 health at spellcraft 1, 1 more for each spellcraft above it.
const BURN: u32 = 0x6F00_7101;
const HEALTH: u32 = 258;
const DEATH_MESSAGE: u32 = 0x8207_002B;

fn gate() -> ContentObject {
    ContentObject {
        object: Some(GATE),
        template: GATE_T,
        description: Description {
            name_text: None,
            package: 0x13A4,
            look: 0x1F00_0297,
            enter_fx: 0,
            scale: None,
            name: Some((0xA860_7CEC, 0x2500_28B6)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: HALL,
        origin: GATE_AT,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

fn burn() -> EffectDef {
    EffectDef {
        kind: EffectKind::InstantVital {
            flags: 0x1,
            change: Scale(vec![(1.0, -10.0), (101.0, -110.0)]),
        },
        ..yalain_testkit::plain_effect(BURN)
    }
}

/// A row of the gate: what it does, when.
fn row(used: bool, touch: Option<f32>, effects: Vec<(u32, f32)>, harm: Option<Harm>) -> Hazard {
    Hazard {
        key: "gate".into(),
        objects: vec![0],
        spots: Vec::new(),
        used,
        touch,
        effects,
        harm,
    }
}

/// The hall with the gate and the world's hazard rows, and a player in it: its client, character
/// and world connection.
fn hall(rows: Vec<Hazard>) -> (Harness, usize, u64, yalain_net::ConnId) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content.objects = vec![gate()];
    h.server.content.hazards = HazardTable { rows };
    h.server.effect_table.effects.insert(BURN, burn());
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    (h, b, me, world)
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// Report the player standing `dx` meters east of the gate.
fn stand(h: &mut Harness, b: usize, dx: f32) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            HALL,
            Vec3::new(GATE_AT[0] + dx, GATE_AT[1], GATE_AT[2]),
            0,
            yalain_world::world::IDLE_MOTION,
            0,
        ),
    );
    run_for(h, 1.0);
}

fn health(h: &Harness, world: yalain_net::ConnId) -> i32 {
    yalain_world::quals::int(&h.server.players[&world].qualities, HEALTH).expect("health")
}

/// A use of the gate, as the client asks for it.
fn use_gate(h: &mut Harness, b: usize, me: u64) {
    use yalain_world::graph::{Builder, Slot};
    let mut g = Builder::new();
    g.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(GATE),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[Arg::Object(g.graph())]),
    );
    run_for(h, 4.0);
}

/// The texts of the death messages a client was told since `from`.
fn deaths(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == DEATH_MESSAGE)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x: &Value| match x.root() {
                Some(Node::TextInfo(t)) => Some((t.string_id, t.table)),
                _ => None,
            })
        })
        .collect()
}

/// The health changes told to a client since `from`.
fn health_sent(h: &Harness, b: usize, from: usize) -> Vec<i32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == HEALTH => Some(q.value),
            _ => None,
        })
        .collect()
}

/// A gate that harms at a touch takes its harm from a player who comes within its reach, not from
/// one outside it; not again while the player stays within, and again once it has left and come
/// back.
/// Behaviour: server.places.a-hazard-harms-whoever-comes-within-its-reach
#[test]
fn a_hazard_harms_a_player_coming_within_its_reach_once_until_it_leaves_and_comes_back() {
    let (mut h, b, _, world) = hall(vec![row(
        false,
        Some(3.0),
        Vec::new(),
        Some(Harm::Points(25)),
    )]);
    stand(&mut h, b, 6.0);
    let full = health(&h, world);
    assert!(full > 60, "{full}");
    stand(&mut h, b, 3.5);
    assert_eq!(health(&h, world), full, "outside its reach");
    let from = h.bots[b].inbox.len();
    stand(&mut h, b, 2.5);
    assert_eq!(health(&h, world), full - 25, "come within it");
    assert_eq!(health_sent(&h, b, from), [full - 25], "told");
    stand(&mut h, b, 1.0);
    assert_eq!(health(&h, world), full - 25, "not again while within");
    stand(&mut h, b, 6.0);
    stand(&mut h, b, 2.0);
    assert_eq!(
        health(&h, world),
        full - 50,
        "again once it left and came back"
    );
}

/// A spot that kills at a touch takes all of a player's health: it dies, by misadventure. Dead, it
/// is harmed by nothing more, though it leaves and comes back.
/// Behaviour: server.places.a-hazard-harms-whoever-comes-within-its-reach
#[test]
fn a_spot_that_kills_takes_all_a_players_health_and_it_dies_by_misadventure() {
    let spot = Hazard {
        key: "floor".into(),
        objects: Vec::new(),
        spots: vec![(HALL, [GATE_AT[0] + 4.0, GATE_AT[1], GATE_AT[2]])],
        used: false,
        touch: Some(2.0),
        effects: vec![(BURN, 1.0)],
        harm: Some(Harm::All),
    };
    let (mut h, b, _, world) = hall(vec![spot]);
    stand(&mut h, b, 8.0);
    let from = h.bots[b].inbox.len();
    stand(&mut h, b, 5.0);
    assert_eq!(health(&h, world), 0, "all its health");
    assert!(h.server.players[&world].dying.is_some(), "it dies");
    assert_eq!(
        deaths(&h, b, from),
        [yalain_world::death::MISADVENTURE],
        "by misadventure"
    );
    let from = h.bots[b].inbox.len();
    stand(&mut h, b, 8.0);
    stand(&mut h, b, 4.5);
    assert!(health_sent(&h, b, from).is_empty(), "nothing more");
    assert!(deaths(&h, b, from).is_empty());
}

/// A gate that harms at a use is used as a person is: whoever uses it is answered, has its effects
/// put on at their spellcraft, and takes its harm. A gate that harms only at a touch does nothing
/// to a use from beyond its reach.
/// Behaviour: server.places.a-hazard-harms-whoever-uses-it
#[test]
fn a_hazard_harms_whoever_uses_it_with_its_effects_and_its_harm() {
    let (mut h, b, me, world) = hall(vec![row(
        true,
        None,
        vec![(BURN, 21.0)],
        Some(Harm::Points(5)),
    )]);
    stand(&mut h, b, 2.0);
    let full = health(&h, world);
    let from = h.bots[b].inbox.len();
    use_gate(&mut h, b, me);
    assert_eq!(
        health(&h, world),
        full - 30 - 5,
        "its effect at 21, then its harm"
    );
    assert!(
        h.functions(b, from)
            .contains(&yalain_world::places::function::USE_DONE),
        "the use answered"
    );

    let (mut h, b, me, world) = hall(vec![row(false, Some(1.0), Vec::new(), Some(Harm::All))]);
    stand(&mut h, b, 2.0);
    let full = health(&h, world);
    use_gate(&mut h, b, me);
    assert_eq!(
        health(&h, world),
        full,
        "a touch's row does nothing at a use"
    );
}

/// A gate kept from the world (an event that is off) harms no one coming within its reach.
/// Behaviour: server.places.a-hazard-harms-whoever-comes-within-its-reach
#[test]
fn a_hazard_kept_from_the_world_harms_no_one() {
    let (mut h, b, _, world) = hall(vec![row(
        false,
        Some(3.0),
        Vec::new(),
        Some(Harm::Points(25)),
    )]);
    h.server.content.events = yalain_content::events::EventTable {
        events: vec![yalain_content::events::Event {
            key: "never".into(),
            game_event: None,
            windows: Vec::new(),
            on_players: Vec::new(),
            failure: None,
            hides: Vec::new(),
        }],
        objects: vec![yalain_content::events::ObjectGate {
            object: 0,
            event: "never".into(),
        }],
        ..Default::default()
    };
    stand(&mut h, b, 6.0);
    let full = health(&h, world);
    stand(&mut h, b, 1.0);
    assert_eq!(health(&h, world), full);
}
