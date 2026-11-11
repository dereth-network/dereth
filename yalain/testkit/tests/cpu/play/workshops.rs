//! Workshops: the effect a workshop puts on the players around it.

use auberean_primitives::Vec3;
use auberean_protocol::{queue, ServerMessage};
use yalain_testkit::Harness;
use yalain_world::calls::{self, Value};
use yalain_world::effects;
use yalain_world::workshops::{self, Workshops};

const FORGING: u32 = 0x6F00_7004;
const WORKSHOP: u64 = 0x4057_6600_0000_0025;

/// The workshop's land cell: x 120 to 140 and y 20 to 40 of its landblock.
const CELL: u32 = 0x5766_0032;
const WORKSHOP_AT: [f32; 3] = [131.1, 36.0, 190.1];

/// A player in the world outdoors, far from the workshop in its landblock; its world connection.
fn near_a_workshop() -> (Harness, usize, yalain_net::ConnId) {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    h.server
        .effect_table
        .effects
        .insert(FORGING, yalain_testkit::plain_effect(FORGING));
    let mut w = Workshops::default();
    w.place(
        WORKSHOP,
        yalain_world::presence::land_cell(CELL, WORKSHOP_AT),
        FORGING,
    );
    h.server.workshops = w;
    (h, b, world)
}

/// Report the player at a point of the workshop's landblock, outdoors or in a building.
fn walk(h: &mut Harness, b: usize, x: f32, y: f32, indoors: bool) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (column, row) = ((x / 20.0) as u32, (y / 20.0) as u32);
    let cell = if indoors {
        0x5766_0100
    } else {
        0x5766_0000 | (column * 8 + row + 1)
    };
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            cell,
            Vec3::new(x, y, 190.5),
            0,
            yalain_world::world::IDLE_MOTION,
            0,
        ),
    );
    h.run(4);
}

fn records(h: &Harness, world: yalain_net::ConnId) -> Vec<yalain_world::state::EffectState> {
    h.server.players[&world]
        .play
        .effects
        .iter()
        .filter(|e| e.state == FORGING)
        .copied()
        .collect()
}

fn calls_of(h: &Harness, b: usize, from: usize, f: u32) -> Vec<Vec<Value>> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == f => Some(
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            ),
            _ => None,
        })
        .collect()
}

/// A workshop's effect comes on a player inside the 3 by 3 land cells around it, cast by the
/// workshop, held until taken off and gone at logout; once, however long it stays.
/// Behaviour: server.craft.workshop-effect-near
#[test]
fn a_workshop_puts_its_effect_on_a_player_within_one_land_cell() {
    let (mut h, b, world) = near_a_workshop();
    walk(&mut h, b, 90.0, 30.0, false);
    assert!(records(&h, world).is_empty(), "two land cells off");
    let from = h.bots[b].inbox.len();
    walk(&mut h, b, 101.0, 59.0, false);
    let r = records(&h, world);
    assert_eq!(r.len(), 1, "the land cell at the corner");
    assert_eq!(
        (
            r[0].caster,
            r[0].item,
            r[0].flags,
            r[0].categories,
            r[0].spellcraft,
            r[0].timeout
        ),
        (WORKSHOP, WORKSHOP, 81, 1, 1.0, f32::MAX)
    );
    assert_eq!(calls_of(&h, b, from, effects::function::ADD).len(), 1);
    walk(&mut h, b, 130.0, 30.0, false);
    assert_eq!(records(&h, world).len(), 1, "not again");
    assert_eq!(calls_of(&h, b, from, effects::function::ADD).len(), 1);
}

/// The record comes off once the player is beyond the land cells around, its client told.
/// Behaviour: server.craft.workshop-effect-off-beyond
#[test]
fn a_workshops_effect_comes_off_a_player_beyond_the_land_cells_around_it() {
    let (mut h, b, world) = near_a_workshop();
    walk(&mut h, b, 130.0, 30.0, false);
    let eid = records(&h, world)[0].eid;
    walk(&mut h, b, 159.0, 59.0, false);
    assert_eq!(records(&h, world).len(), 1, "still within");
    let from = h.bots[b].inbox.len();
    walk(&mut h, b, 130.0, 61.0, false);
    assert!(records(&h, world).is_empty(), "two land cells off");
    assert_eq!(
        calls_of(&h, b, from, effects::function::REMOVE),
        [vec![Value::Int(eid)]]
    );
}

/// Indoors nothing changes: a player keeps the record inside a building, however far in, and gets
/// none it did not hold; coming out beyond the land cells around, it goes.
/// Behaviour: server.craft.workshop-effect-indoors
#[test]
fn indoors_a_workshops_effect_neither_comes_nor_goes() {
    let (mut h, b, world) = near_a_workshop();
    walk(&mut h, b, 130.0, 30.0, true);
    assert!(records(&h, world).is_empty(), "none comes indoors");
    walk(&mut h, b, 130.0, 30.0, false);
    assert_eq!(records(&h, world).len(), 1);
    walk(&mut h, b, 30.0, 30.0, true);
    assert_eq!(records(&h, world).len(), 1, "held indoors");
    walk(&mut h, b, 30.0, 30.0, false);
    assert!(records(&h, world).is_empty(), "out, and beyond");
}

/// A player's record comes off when the server is told it has gone to a far part of the world,
/// even indoors.
/// Behaviour: server.craft.workshop-effect-off-far
#[test]
fn a_player_who_goes_indoors_far_away_loses_a_workshops_effect() {
    let (mut h, b, world) = near_a_workshop();
    walk(&mut h, b, 130.0, 30.0, false);
    assert_eq!(records(&h, world).len(), 1);
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            0x6EA8_0100,
            Vec3::new(30.0, 30.0, 100.0),
            0,
            yalain_world::world::IDLE_MOTION,
            0,
        ),
    );
    h.run(4);
    assert!(records(&h, world).is_empty());
}

/// A player entering the world within a workshop's land cells has its record at once, standing
/// still; the record it held before leaving did not come with it.
/// Behaviour: server.craft.workshop-effect-at-entry
#[test]
fn a_player_entering_beside_a_workshop_gets_its_effect() {
    let (mut h, b, world) = near_a_workshop();
    walk(&mut h, b, 130.0, 30.0, false);
    assert_eq!(records(&h, world).len(), 1);
    let eid = records(&h, world)[0].eid;
    let me = h.server.players[&world].character;
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &auberean_protocol::ClientMessage::LogOff(auberean_protocol::login::CharacterRef {
            character: me,
        }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    h.enter_world(b, "one", me);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world again")
        .0;
    let r = records(&h, world);
    assert_eq!(r.len(), 1, "once");
    assert_ne!(r[0].eid, eid, "a new record");
}

/// Among the placed objects, a template with the effect's property is a workshop, standing where
/// it was placed.
/// Behaviour: server.craft.workshops-from-their-templates
#[test]
fn a_placed_object_whose_template_names_an_effect_is_a_workshop() {
    let (mut h, _, _) = near_a_workshop();
    let mut props = yalain_content::Props::default();
    props.set(
        workshops::EFFECT_PROPERTY,
        yalain_content::Prop::Word(FORGING),
    );
    h.server
        .catalog
        .insert(yalain_testkit::test_template(0x4700_7003, 0xA21, props));
    let o = yalain_content::ContentObject {
        object: Some(WORKSHOP),
        template: 0x4700_7003,
        description: yalain_content::Description {
            name_text: None,
            package: 0xA21,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: CELL,
        origin: WORKSHOP_AT,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: yalain_content::Kind::Static,
        label: String::new(),
        placed: String::new(),
        destination: None,
        facts: yalain_content::Facts::default(),
        walks: None,
        gate: None,
    };
    let w = Workshops::of(&[o], &h.server.catalog);
    // Its cell is the seventh along x, the second along y, of landblock 0x57, 0x66.
    let (x, y) = (0x57 * 8 + 6, 0x66 * 8 + 1);
    assert_eq!(
        w.reaching((x + 1, y + 1))
            .iter()
            .map(|k| (k.id, k.effect))
            .collect::<Vec<_>>(),
        [(WORKSHOP, FORGING)]
    );
    assert!(w.reaching((x + 2, y)).is_empty());
}

/// Three rooms of the workshop's landblock: the workshop's own, with a doorway to the outdoors and
/// one to the next room; the next room; and a room beside the workshop's, behind a wall, with its
/// own doorway to the outdoors.
const ROOM: u32 = 0x5766_0110;
const NEXT_ROOM: u32 = 0x5766_0111;
const WALLED_ROOM: u32 = 0x5766_0112;
const FORGE_AT: [f32; 3] = [50.0, 50.0, 5.0];

/// A player in the world outdoors, a workshop standing in a room of its landblock, reaching 8 m;
/// its world connection.
fn near_a_workshop_indoors() -> (Harness, usize, yalain_net::ConnId) {
    let (mut h, b, world) = near_a_workshop();
    let mut w = Workshops::default();
    w.place_indoors(workshops::IndoorWorkshop {
        id: WORKSHOP,
        cell: ROOM,
        origin: FORGE_AT,
        effect: FORGING,
        reach: 8.0,
    });
    h.server.workshops = w;
    h.server.ground = Some(Box::new(
        yalain_testkit::TestGround::default()
            .with_room(
                ROOM,
                &[yalain_world::reported::OUTDOORS, NEXT_ROOM],
                &[NEXT_ROOM],
            )
            .with_room(NEXT_ROOM, &[ROOM], &[ROOM])
            .with_room(WALLED_ROOM, &[yalain_world::reported::OUTDOORS], &[]),
    ));
    (h, b, world)
}

/// Report the player at a point of a room of the workshop's landblock.
fn walk_in(h: &mut Harness, b: usize, cell: u32, x: f32, y: f32) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            cell,
            Vec3::new(x, y, FORGE_AT[2]),
            0,
            yalain_world::world::IDLE_MOTION,
            0,
        ),
    );
    h.run(4);
}

/// A workshop standing indoors puts its effect on a player within its reach in its own room or one
/// its doorways join, none on one behind a wall, and takes it off beyond its reach and outdoors.
/// Behaviour: server.craft.workshop-indoors-reaches-its-rooms
#[test]
fn a_workshop_indoors_reaches_players_within_its_reach_in_the_rooms_it_joins() {
    let (mut h, b, world) = near_a_workshop_indoors();
    walk_in(&mut h, b, WALLED_ROOM, 50.0, 56.0);
    assert_eq!(h.server.players[&world].position.cell, WALLED_ROOM);
    assert!(records(&h, world).is_empty(), "6 m off, behind a wall");
    walk(&mut h, b, 50.0, 50.0, false);
    assert!(records(&h, world).is_empty(), "outdoors above it");
    let from = h.bots[b].inbox.len();
    walk_in(&mut h, b, ROOM, 50.0, 55.0);
    let r = records(&h, world);
    assert_eq!(r.len(), 1, "5 m off in its room");
    assert_eq!(
        (r[0].caster, r[0].item, r[0].flags, r[0].categories),
        (WORKSHOP, WORKSHOP, 81, 1)
    );
    walk_in(&mut h, b, NEXT_ROOM, 50.0, 57.5);
    assert_eq!(h.server.players[&world].position.cell, NEXT_ROOM);
    assert_eq!(records(&h, world).len(), 1, "7.5 m off in the next room");
    assert_eq!(calls_of(&h, b, from, effects::function::ADD).len(), 1);
    walk_in(&mut h, b, NEXT_ROOM, 50.0, 58.5);
    assert!(records(&h, world).is_empty(), "8.5 m off");
    walk_in(&mut h, b, ROOM, 50.0, 54.0);
    assert_eq!(records(&h, world).len(), 1, "back within");
    walk(&mut h, b, 50.0, 54.0, false);
    assert!(records(&h, world).is_empty(), "outdoors");
}

/// A placed workshop in an interior cell reaches as far as its template says, 8 m where it says
/// nothing, in its own landblock alone and no land cell outdoors.
/// Behaviour: server.craft.workshops-indoors-from-their-templates
#[test]
fn a_workshop_placed_indoors_reaches_as_far_as_its_template_says() {
    let (mut h, _, _) = near_a_workshop();
    let template = |id: u32, reach: Option<f32>| {
        let mut props = yalain_content::Props::default();
        props.set(
            workshops::EFFECT_PROPERTY,
            yalain_content::Prop::Word(FORGING),
        );
        if let Some(r) = reach {
            props.set(workshops::REACH_PROPERTY, yalain_content::Prop::Float(r));
        }
        yalain_testkit::test_template(id, 0xA21, props)
    };
    h.server.catalog.insert(template(0x4700_7003, Some(10.0)));
    h.server.catalog.insert(template(0x4700_7004, None));
    let placed = |id: u64, template: u32, origin: [f32; 3]| yalain_content::ContentObject {
        object: Some(id),
        template,
        description: yalain_content::Description {
            name_text: None,
            package: 0xA21,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: ROOM,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: yalain_content::Kind::Static,
        label: String::new(),
        placed: String::new(),
        destination: None,
        facts: yalain_content::Facts::default(),
        walks: None,
        gate: None,
    };
    let w = Workshops::of(
        &[
            placed(WORKSHOP, 0x4700_7003, FORGE_AT),
            placed(WORKSHOP + 1, 0x4700_7004, [100.0, 50.0, 5.0]),
        ],
        &h.server.catalog,
    );
    let at = |cell: u32, x: f32| yalain_store::StoredPosition {
        cell,
        origin: [x, 50.0, 5.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let reached = |p: &yalain_store::StoredPosition| -> Vec<(u64, u32, f32)> {
        w.reaching_indoors(None, p)
            .iter()
            .map(|k| (k.id, k.effect, k.reach))
            .collect()
    };
    assert_eq!(reached(&at(ROOM, 59.5)), [(WORKSHOP, FORGING, 10.0)]);
    assert!(reached(&at(ROOM, 60.5)).is_empty(), "beyond its 10 m");
    assert_eq!(reached(&at(ROOM, 92.5)), [(WORKSHOP + 1, FORGING, 8.0)]);
    assert!(reached(&at(ROOM, 91.5)).is_empty(), "beyond 8 m");
    assert!(
        reached(&at(0x5767_0110, 50.0)).is_empty(),
        "another landblock's room"
    );
    let land = yalain_world::presence::land_cell(ROOM, FORGE_AT);
    assert!(w.reaching(land).is_empty(), "no land cell outdoors");
}
