//! Attacks and skills aimed at people: a placed person or object the player talks to is refused as
//! one a skill or a basic attack is not used on (22014), after the checks before, and so is a run
//! moved on to one; a creature that holds a person's table (a barrel) is fought as any creature; a
//! target gone is refused as gone.
//! Fixture: the test world's camp beside the start, with a person placed beside the player here.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_CREATURE};
use yalain_world::calls::{self, Value};
use yalain_world::combat::error;
use yalain_world::skill_use::{function as skill, refusal};

const START_ATTACK: u32 = 0x8204_0008;
const BASIC_ATTACKS_FAILED: u32 = 0x8204_001C;
const SWORD: u32 = 0x4700_051C;
const BOW: u32 = 0x4700_051B;
const DEADEYE: u32 = 207;
/// A person's template, made up for the test: a person's table, no creature.
const PERSON: u32 = 0x4700_FE01;
/// A talking object's template: of the script library's person kind, no table.
const TALKER: u32 = 0x4700_FE02;
/// The package the person kind is here.
const PERSON_PACKAGE: u32 = 0x10F0;

/// A player in the world with the scene drawn, beside the camp's beetle, a weapon in hand, and a
/// person placed of `template` a meter off; returns the harness, its client, the player, the
/// person's id and the beetle.
fn beside_a_person(weapon: u32, template: u32, table: bool) -> (Harness, usize, u64, u64, u64) {
    let mut h = Harness::new();
    h.add_creatures(1);
    let mut props = Props::default();
    if table {
        props.set("AI_NPCTable", Prop::Text("NPCTable_ForATest".into()));
    }
    let package = if table { 0x1067 } else { PERSON_PACKAGE };
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, package, props));
    h.server.person_packages.insert(PERSON_PACKAGE);
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, weapon);
    h.all_hold(DEADEYE);
    let here = h
        .server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in the world")
        .position;
    h.server
        .content
        .objects
        .push(yalain_content::ContentObject {
            object: None,
            template,
            description: yalain_content::Description {
                name_text: None,
                package,
                look: 0x1F00_0100,
                enter_fx: 0,
                scale: None,
                name: Some((0xA860_7CEC, 0x2500_1B44)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: here.cell,
            origin: [here.origin[0] + 1.0, here.origin[1], here.origin[2]],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: yalain_content::Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: yalain_content::Facts::default(),
            walks: None,
            gate: None,
        });
    let objects = &h.server.content.objects;
    let person = yalain_world::presence::content_id(objects, objects.len() - 1);
    let beetle = h
        .world_messages(b, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.weenie.entity_did == TEST_CREATURE => Some(o.id),
            _ => None,
        })
        .expect("the camp's creature");
    (h, b, me, person, beetle)
}

/// The calls a client received since `from`: each function and its whole-number values.
fn calls_since(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<u32>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => {
                let v = c
                    .args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default();
                let ints = v
                    .iter()
                    .filter_map(|x| match x {
                        Value::Int(i) => Some(*i),
                        _ => None,
                    })
                    .collect();
                Some((c.function.0, ints))
            }
            _ => None,
        })
        .collect()
}

/// Start an attack run on a target; the run's failures since.
fn run_at(h: &mut Harness, b: usize, target: u64) -> Vec<u32> {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(START_ATTACK, &[Arg::LongInt(target)]),
    );
    h.run(6);
    calls_since(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == BASIC_ATTACKS_FAILED)
        .filter_map(|(_, v)| v.first().copied())
        .collect()
}

/// Use Deadeye on a target; the use's refusals since (why).
fn special_at(h: &mut Harness, b: usize, target: u64) -> Vec<u32> {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            skill::USE,
            &[Arg::Int(4), Arg::Int(DEADEYE), Arg::LongInt(target)],
        ),
    );
    h.run(6);
    calls_since(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == skill::REFUSED)
        .filter_map(|(_, v)| v.iter().copied().find(|x| *x > 1000 && *x != DEADEYE))
        .collect()
}

/// An attack run at a person is refused 22014, as one basic attacks are not used on.
/// Behaviour: server.combat.run-at-a-person-refused
#[test]
fn an_attack_run_at_a_person_is_refused_as_not_used_on_npcs() {
    let (mut h, b, _, person, _) = beside_a_person(SWORD, PERSON, true);
    assert_eq!(run_at(&mut h, b, person), vec![error::CANNOT_USE_ON_NPC]);
}

/// A special at a person is refused 22014; asked on the move it is refused for moving first.
/// Behaviour: server.specials.at-a-person-refused
#[test]
fn a_special_at_a_person_is_refused_as_not_used_on_npcs_after_the_moving_check() {
    let (mut h, b, me, person, _) = beside_a_person(BOW, PERSON, true);
    assert_eq!(special_at(&mut h, b, person), vec![refusal::ON_NPC]);
    let w = yalain_world::social::world_of(&h.server, me).expect("in the world");
    if let Some(p) = h.server.players.get_mut(&w) {
        p.heading_motion = yalain_world::world::motion(127, 255, 127);
    }
    assert_eq!(special_at(&mut h, b, person), vec![refusal::MOVING]);
}

/// A placed object of the person kind, with no table of its own, is talked to and refused 22014 too.
/// Behaviour: server.combat.run-at-a-person-refused
#[test]
fn an_attack_run_at_a_talking_object_is_refused_as_not_used_on_npcs() {
    let (mut h, b, _, talker, _) = beside_a_person(SWORD, TALKER, false);
    assert_eq!(run_at(&mut h, b, talker), vec![error::CANNOT_USE_ON_NPC]);
}

/// A creature that holds a person's table (as the barrels do) is fought as any creature: the run
/// goes, refused nothing; a target gone is refused as gone (20002).
/// Behaviour: server.combat.run-at-a-person-refused
#[test]
fn a_creature_with_a_persons_table_is_fought_and_a_gone_target_is_refused_as_gone() {
    let (mut h, b, _, _, beetle) = beside_a_person(SWORD, PERSON, true);
    let template = h
        .server
        .things
        .get(beetle)
        .map(|t| t.template)
        .expect("the beetle");
    let mut props = Props::default();
    props.set("AI_NPCTable", Prop::Text("NPCTable_Barrier".into()));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, 0x1067, props));
    assert_eq!(run_at(&mut h, b, beetle), Vec::<u32>::new(), "fought");
    assert_eq!(
        run_at(&mut h, b, 0x7FFF_0000_0000_0001),
        vec![error::TARGET_GONE]
    );
}

/// A run under way moved on to a person (the client naming it as the next target) is refused as
/// one a basic attack is not used on, and ends.
/// Behaviour: server.player-fights.a-run-moved-onto-a-person-is-refused
#[test]
fn a_run_moved_onto_a_person_is_refused_as_not_used_on_one() {
    const SET_NEXT_TARGET: u32 = 0x8204_0001;
    let (mut h, b, _, person, beetle) = beside_a_person(SWORD, PERSON, true);
    assert!(run_at(&mut h, b, beetle).is_empty(), "the run begins");
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(SET_NEXT_TARGET, &[Arg::LongInt(person)]),
    );
    h.run(80);
    let told: Vec<(u32, Vec<u32>)> = calls_since(&h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == BASIC_ATTACKS_FAILED || *f == 0x8204_001B)
        .collect();
    assert_eq!(
        told,
        [
            (BASIC_ATTACKS_FAILED, vec![error::CANNOT_USE_ON_NPC]),
            (0x8204_001B, vec![0])
        ]
    );
}
