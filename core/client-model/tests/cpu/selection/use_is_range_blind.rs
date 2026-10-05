//! Contracts for use is range blind.
//! Fixture: shared recorded messages and synthetic state.

use super::common::corpus;

use std::collections::BTreeSet;

use corpus::Dir;
use dereth_client_model::inventory::use_object::{ItemUses, UseOutcome, UseResult};
use dereth_client_model::inventory::SplitState;
use dereth_client_model::{RecordingRequests, RecordingSink, Request, World};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::movement::{movement_type, MoveToArm, MovementParameters as WireParams};
use dereth_protocol::types::PublicWeenieDesc;
use dereth_protocol::{Message, Reader};
use {dereth_rules::weenie::bitfield, dereth_rules::weenie::item_type};

const USEABLE_REMOTE: u32 = 0x20;
const USEABLE_NO: u32 = 0x01;

const OP_SET_OBJECT_MOVEMENT: u32 = 0xF74C;
const OP_CHARACTER_SET: u32 = 0xF658;

const ACADEMY_USE_RADIUS: f32 = 3.0;
const GREETER_AT: (f32, f32, f32) = (9.836_07, -31.7347, 0.005);
const JONATHAN_AT: (f32, f32, f32) = (22.1139, -19.142, 0.005);

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const GREETER: ObjectId = ObjectId(0x5000_0010);
const JONATHAN: ObjectId = ObjectId(0x5000_0011);
const MUTE: ObjectId = ObjectId(0x5000_0012);

#[test]
fn the_server_sends_the_approach_and_names_the_targets_own_radius() {
    let mut arms = [0usize; 10];
    let mut player_radii: BTreeSet<String> = BTreeSet::new();
    let mut player_move_tos = 0usize;

    for (name, session) in corpus::load_all() {
        let mut characters: BTreeSet<ObjectId> = BTreeSet::new();
        for b in &session {
            if b.dir == Dir::S2c && b.opcode() == OP_CHARACTER_SET {
                let mut r = Reader::new(b.body());
                if let Ok(set) = dereth_protocol::login::LoginCharacterSet::read(&mut r) {
                    characters.extend(set.characters.iter().map(|c| c.gid));
                }
            }
        }
        for b in &session {
            if b.dir != Dir::S2c || b.opcode() != OP_SET_OBJECT_MOVEMENT {
                continue;
            }
            let mut r = Reader::new(b.body());
            let msg = dereth_protocol::movement::MovementSetObjectMovement::read(&mut r)
                .unwrap_or_else(|e| panic!("{name}: a 0xF74C does not decode: {e:?}"));
            let buf = msg
                .decoded_movement()
                .unwrap_or_else(|e| panic!("{name}: its movement buffer does not decode: {e:?}"));
            if buf.body.movement_type == movement_type::INVALID {
                continue;
            }
            arms[buf.body.movement_type as usize] += 1;
            if buf.body.movement_type != movement_type::MOVE_TO_OBJECT
                || !characters.contains(&msg.id)
            {
                continue;
            }
            let Some(MoveToArm::MoveToObject { params, .. }) =
                buf.body.decode_move_to().expect("a type 6 decodes")
            else {
                panic!("{name}: type 6 decoded as another arm")
            };
            let WireParams::MoveTo {
                distance_to_object, ..
            } = params
            else {
                panic!("{name}: a MoveToObject carries the 0x1C parameter form")
            };
            assert!(
                !buf.autonomous,
                "{name}: a server-driven approach is never autonomous"
            );
            player_radii.insert(format!("{distance_to_object}"));
            player_move_tos += 1;
        }
    }

    assert!(arms[6..=9].iter().all(|n| *n > 0));
    assert!(arms.iter().sum::<usize>() >= player_move_tos);

    assert!(player_move_tos > 0);
    assert!(
        player_radii.len() > 1,
        "approach radius comes from the target, not a constant"
    );
    assert!(
        player_radii.contains("3"),
        "the 3 m radius wanted is in the corpus"
    );
}

fn put(w: &mut World, id: ObjectId, pwd: PublicWeenieDesc) {
    let mut it = dereth_client_model::Weenie::new(id);
    it.pwd = pwd;
    it.valid = true;
    w.tables.weenies.insert(id, it);
}

fn academy_npc(w: &mut World, id: ObjectId, useable: u32) {
    put(
        w,
        id,
        PublicWeenieDesc {
            obj_type: item_type::CREATURE,
            bitfield: bitfield::STUCK,
            useability: Some(useable),
            use_radius: Some(ACADEMY_USE_RADIUS),
            ..PublicWeenieDesc::default()
        },
    );
}

fn academy_world() -> World {
    let mut w = World::new();
    put(
        &mut w,
        PLAYER,
        PublicWeenieDesc {
            bitfield: bitfield::PLAYER,
            obj_type: item_type::CREATURE,
            items_capacity: Some(102),
            containers_capacity: Some(7),
            ..PublicWeenieDesc::default()
        },
    );
    w.player = Some(PLAYER);
    w.tables.inventories.insert(
        PLAYER,
        dereth_client_model::objects::ObjectInventory::new(PLAYER),
    );
    academy_npc(&mut w, GREETER, USEABLE_REMOTE);
    academy_npc(&mut w, JONATHAN, USEABLE_REMOTE);
    academy_npc(&mut w, MUTE, USEABLE_NO);
    w
}

fn click(w: &mut World, id: ObjectId, t: f64) -> (UseOutcome, Vec<Request>) {
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    let outcome = w.use_object(&mut req, &mut out, id, SplitState::default(), ServerTime(t));
    (outcome, req.0)
}

fn use_event_target(r: &Request) -> Option<ObjectId> {
    match r {
        Request::UseEvent(m) => Some(m.object),
        _ => None,
    }
}

/// Behaviour: use.range.the-client-sends-a-use-at-any-range-and-the-shard-sends-the-approach
#[test]
fn a_use_is_range_blind_because_range_is_not_an_input() {
    let mut w = academy_world();

    let ground_before = w.ground_object;
    let (outcome, reqs) = click(&mut w, GREETER, 10.0);
    assert_eq!(
        outcome,
        UseOutcome::UseEventSent,
        "a remote-useable creature is used, not carried"
    );
    assert_eq!(
        reqs.iter().filter_map(use_event_target).collect::<Vec<_>>(),
        vec![GREETER],
        "exactly one 0x0036, naming the greeter"
    );
    assert_eq!(reqs.len(), 1, "and nothing else goes out: {reqs:?}");
    assert_eq!(
        w.ground_object, ground_before,
        "no local state change at all"
    );

    let (outcome2, reqs2) = click(&mut w, JONATHAN, 10.5);
    assert_eq!(outcome2, outcome, "the far NPC takes the identical arm");
    assert_eq!(
        reqs2
            .iter()
            .filter_map(use_event_target)
            .collect::<Vec<_>>(),
        vec![JONATHAN],
        "and the identical request, naming Jonathan"
    );
    assert_eq!(w.ground_object, ground_before);

    let d = distance(GREETER_AT, JONATHAN_AT);
    assert!(
        (d - 17.588).abs() < 0.001,
        "the two instances are {d:.3} m apart"
    );
    assert!(
        d - ACADEMY_USE_RADIUS > 14.5,
        "from inside the greeter's radius, Jonathan is {:.3} m outside his own",
        d - ACADEMY_USE_RADIUS
    );
}

fn distance(a: (f32, f32, f32), b: (f32, f32, f32)) -> f32 {
    let (dx, dy, dz) = (b.0 - a.0, b.1 - a.1, b.2 - a.2);
    dz.mul_add(dz, dx.mul_add(dx, dy * dy)).sqrt()
}

#[test]
fn the_two_academy_npcs_take_the_same_arm() {
    let mut w = academy_world();

    assert_eq!(w.determine_use_result(GREETER), UseResult::Useable);
    assert_eq!(w.determine_use_result(JONATHAN), UseResult::Useable);
    assert_eq!(
        w.determine_use_result(GREETER),
        w.determine_use_result(JONATHAN),
        "nothing in either descriptor selects a different arm"
    );
    assert!(!UseResult::Useable.takes_the_fast_path());

    let (mute, mute_reqs) = click(&mut w, MUTE, 20.0);
    assert_eq!(
        mute,
        UseOutcome::Refused(dereth_client_model::inventory::use_object::UseRefusal::NotUseable)
    );
    assert!(
        mute_reqs.iter().all(|r| use_event_target(r).is_none()),
        "USEABLE_NO sends no 0x0036: {mute_reqs:?}"
    );
    assert!(ItemUses(USEABLE_REMOTE).is_useable());
    assert!(!ItemUses(USEABLE_NO).is_useable());
    assert!(
        !ItemUses(USEABLE_REMOTE).is_useable_targeted(),
        "0x20 is a source flag; nothing arms the second-click target mode"
    );
}

#[test]
fn the_use_radius_arrives_and_has_no_reader_but_this_one() {
    let w = academy_world();
    for id in [GREETER, JONATHAN] {
        let r = w.weenie(id).and_then(|x| x.pwd.use_radius);
        assert_eq!(
            r,
            Some(ACADEMY_USE_RADIUS),
            "{id:?} carries its 3 m use radius"
        );
    }
}
