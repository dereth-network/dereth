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
