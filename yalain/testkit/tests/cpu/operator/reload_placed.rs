//! The world database's placed objects taken again while the server runs: a new object is made
//! for the players who see it; one moved or changed keeps its id and is made again where it now
//! stands, replacing the one its clients hold; one taken away is forgotten by every client that
//! holds it; a person walking its route that the database left alone walks on; a chest or a door
//! in use is let go first. Fixture: the test hall with people, a chest and a door beside the start,
//! all authored for the tests, and players in the world with their scenes drawn.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::movement::{Gait, Shape, Waypoint};
use yalain_content::{
    CampMoves, ContentObject, ContentPack, Description, Facts, Kind, LootProfile, Prop, Props,
    Route,
};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::presence::content_id;
use yalain_world::reload::table;
use yalain_world::usable::doors;

use super::reload::{reload, tables_of};

const CELL: u32 = 0x0299_0109;
/// A cell of another landblock: out of every player's view.
const FAR: u32 = 0x0300_0109;
const PERSON: u32 = 0x4700_31E6;
const OTHER: u32 = 0x4700_3183;
const NAME_TABLE: u32 = 0x2500_1B44;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const OPEN_DONE: u32 = 0x820E_005D;
const CLOSE_DONE: u32 = 0x820E_0060;

/// A person the database numbers itself.
fn someone(template: u32, name: u32, cell: u32, origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: None,
        template,
        description: Description {
            name_text: None,
            package: 0x13A4,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((name, NAME_TABLE)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Npc,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

/// The hall with these objects, as the server started with them, and a player per account in the
/// world beside the start, each with its scene drawn.
fn hall(objects: Vec<ContentObject>, accounts: &[&str]) -> (Harness, Vec<usize>) {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.placed.loaded = Some(objects.clone());
    h.server.content = ContentPack {
        objects,
        ..ContentPack::default()
    };
    let mut bots = Vec::new();
    for (i, a) in accounts.iter().enumerate() {
        let (b, _) = h.player_in_world(a, &format!("Elara{}", ["", "b", "c"][i]));
        h.bots[b].send(
            true,
            queue::WEENIE,
            &auberean_protocol::ClientMessage::SceneRenderingComplete(
                auberean_protocol::login::Empty,
            ),
        );
        bots.push(b);
    }
    h.run(5);
    (h, bots)
}

/// The objects a client was made since `from`: id, instance stamp, where (cell, x).
fn made(h: &Harness, b: usize, from: usize) -> Vec<(u64, u16, u32, f32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) => Some((
                o.id,
                o.physics.instance_stamp,
                o.physics.position.cell,
                o.physics.position.frame.origin.x,
            )),
            _ => None,
        })
        .collect()
}

/// The objects a client was told to forget since `from`, with the instance stamps.
fn forgotten(h: &Harness, b: usize, from: usize) -> Vec<(u64, u16)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::ForgetObject(o) => Some((o.0.id, o.0.instance_stamp)),
            _ => None,
        })
        .collect()
}

fn ids(h: &Harness) -> Vec<u64> {
    (0..h.server.content.objects.len())
        .map(|i| content_id(&h.server.content.objects, i))
        .collect()
}

/// A reload whose database places two new people, one in the hall and one in another landblock:
/// the player is made the one it sees, under an id of its own, and nothing of the person already
/// there; the far one is made for nobody.
/// Behaviour: server.operator.reload-makes-new-placed-objects
#[test]
fn a_new_placed_object_is_made_for_the_players_who_see_it() {
    let (mut h, b) = hall(
        vec![someone(PERSON, 0xA1, CELL, [61.0, 241.0, -44.6])],
        &["one"],
    );
    let before = ids(&h);
    let from = h.bots[b[0]].inbox.len();
    let mut t = tables_of(&h);
    t.content
        .objects
        .push(someone(OTHER, 0xA2, CELL, [62.0, 243.0, -44.6]));
    t.content
        .objects
        .push(someone(OTHER, 0xA3, FAR, [62.0, 243.0, -44.6]));
    let r = reload(&mut h, t);
    h.run(5);

    assert_eq!(r.applied, [table::PLACED]);
    assert_eq!(
        r.details,
        ["placed objects: 2 new, 0 moved, 0 changed, 0 taken away"]
    );
    let now = ids(&h);
    assert_eq!(now[0], before[0], "the person there keeps its id");
    assert!(now[1] != now[0] && now[2] != now[0] && now[1] != now[2]);
    assert_eq!(
        made(&h, b[0], from),
        [(now[1], 1, CELL, 62.0)],
        "the near one made, nothing else"
    );
}

/// A reload whose database moves one person within the hall, another out of the landblock, and
/// gives a third a new name: each keeps its id; the one still seen and the renamed one are made
/// again with a newer instance stamp (replacing the ones the client holds), the moved one where it
/// now stands; the one gone from view is forgotten under the stamp the client holds.
/// Behaviour: server.operator.reload-places-moved-objects-again
#[test]
fn a_moved_or_changed_placed_object_keeps_its_id_and_is_made_again_where_it_stands() {
    let people = vec![
        someone(PERSON, 0xA1, CELL, [61.0, 241.0, -44.6]),
        someone(PERSON, 0xA2, CELL, [62.0, 243.0, -44.6]),
        someone(OTHER, 0xA3, CELL, [63.0, 244.0, -44.6]),
    ];
    let (mut h, b) = hall(people, &["one"]);
    let before = ids(&h);
    let from = h.bots[b[0]].inbox.len();
    let mut t = tables_of(&h);
    t.content.objects[0].origin = [64.0, 241.0, -44.6];
    t.content.objects[1].cell = FAR;
    t.content.objects[2].description.name = Some((0xB3, NAME_TABLE));
    let r = reload(&mut h, t);
    h.run(5);

    assert_eq!(
        r.details,
        ["placed objects: 0 new, 2 moved, 1 changed, 0 taken away"]
    );
    assert_eq!(ids(&h), before, "every one keeps its id");
    let mut seen = made(&h, b[0], from);
    seen.sort_by_key(|m| m.0);
    assert_eq!(
        seen,
        [(before[0], 2, CELL, 64.0), (before[2], 2, CELL, 63.0)],
        "made again with a newer instance stamp, where each stands"
    );
    assert_eq!(forgotten(&h, b[0], from), [(before[1], 1)]);
    let held = &h.server.players.values().next().expect("in").known;
    assert!(held.contains_key(&before[0]) && !held.contains_key(&before[1]));
}

/// A reload whose database no longer places the first of two people: both players who hold it
/// forget it; the other keeps the id it had, though it is first in the list now, and is sent
/// nothing.
/// Behaviour: server.operator.reload-takes-away-removed-objects
#[test]
fn a_placed_object_taken_away_is_forgotten_by_every_client_that_holds_it() {
    let people = vec![
        someone(PERSON, 0xA1, CELL, [61.0, 241.0, -44.6]),
        someone(PERSON, 0xA2, CELL, [62.0, 243.0, -44.6]),
    ];
    let (mut h, b) = hall(people, &["one", "two"]);
    let before = ids(&h);
    let froms: Vec<usize> = b.iter().map(|b| h.bots[*b].inbox.len()).collect();
    let mut t = tables_of(&h);
    t.content.objects.remove(0);
    let r = reload(&mut h, t);
    h.run(5);

    assert_eq!(
        r.details,
        ["placed objects: 0 new, 0 moved, 0 changed, 1 taken away"]
    );
    assert_eq!(ids(&h), [before[1]], "the one left keeps its id");
    for (b, from) in b.iter().zip(froms) {
        assert_eq!(forgotten(&h, *b, from), [(before[0], 1)]);
        assert!(made(&h, *b, from).is_empty());
    }
    assert!(h
        .server
        .players
        .values()
        .all(|p| !p.known.contains_key(&before[0])));
}

/// The position stamps a client was told a person's places with since `from`.
fn walked(h: &Harness, b: usize, from: usize, id: u64) -> Vec<u16> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::Position(p) | ServerMessage::PositionCell(p) if p.sender.id == id => {
                Some(p.position_stamp)
            }
            _ => None,
        })
        .collect()
}

/// A reload whose database places someone new ahead of a person walking its route, the person's
/// own row as it was: the person is not made again, stays where it has walked to, and walks on,
/// its places told with stamps going on from those before.
/// Behaviour: server.operator.reload-leaves-unchanged-objects-alone
#[test]
fn a_person_walking_its_route_walks_on_through_a_reload_that_left_it_alone() {
    let (a, z) = ([60.0, 239.0, -44.6], [60.0, 249.0, -44.6]);
    let mut walker = someone(OTHER, 0xA1, CELL, a);
    walker.walks = Some(CampMoves {
        route: Some(Route {
            key: "pacing".into(),
            shape: Shape::BackAndForth,
            gait: Gait::Walk,
            speed: None,
            waypoints: vec![
                Waypoint {
                    cell: CELL,
                    at: a,
                    rest: 4.0,
                },
                Waypoint {
                    cell: CELL,
                    at: z,
                    rest: 3.0,
                },
            ],
        }),
        start: 1,
        ..CampMoves::default()
    });
    let (mut h, b) = hall(vec![walker], &["one"]);
    let id = ids(&h)[0];
    let from = h.bots[b[0]].inbox.len();
    assert!(
        h.run_until(200, |h| h.server.content.objects[0].origin[1] > 243.0),
        "it walks"
    );
    let stamps = walked(&h, b[0], from, id);
    let there = h.server.content.objects[0].origin;

    let from = h.bots[b[0]].inbox.len();
    let mut t = tables_of(&h);
    t.content.objects = vec![
        someone(PERSON, 0xA2, CELL, [62.0, 243.0, -44.6]),
        h.server.placed.loaded.clone().expect("loaded")[0].clone(),
    ];
    reload(&mut h, t);
    assert_eq!(ids(&h)[1], id, "it keeps its id");
    assert_eq!(h.server.content.objects[1].origin, there, "where it walked");
    assert!(
        h.run_until(200, |h| h.server.content.objects[1].origin[1] > 248.5),
        "it walks on to the far end"
    );
    assert!(made(&h, b[0], from).iter().all(|m| m.0 != id));
    let after = walked(&h, b[0], from, id);
    let last = *stamps.last().expect("told before");
    assert!(
        !after.is_empty() && after.iter().all(|s| *s > last),
        "{stamps:?} then {after:?}"
    );
}

// --- In use --------------------------------------------------------------------------------------

const CHEST_ID: u64 = 0x4002_9900_0000_0081;
const CHEST: u32 = 0x4700_0F4B;
const CHEST_PACKAGE: u32 = 0x0E8B;
const PROFILE: u32 = 0x3F0;
const CRYSTAL: u32 = 0x4700_0F21;
const DOOR_ID: u64 = 0x4002_9900_0000_0071;
const DOOR: u32 = 0x4700_0FCE;
const DOOR_PACKAGE: u32 = 0x0EB2;

fn standing(object: u64, template: u32, package: u32, origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(object),
        template,
        description: Description {
            name_text: None,
            package,
            look: 0x1F00_0F58,
            enter_fx: 0,
            scale: None,
            name: Some((0x1C06_0002, 0x2500_0002)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        kind: Kind::Static,
        ..someone(template, 0, CELL, origin)
    }
}

/// A use of an object, as the client asks for it.
fn use_of(item: u64, me: u64) -> Arg {
    use yalain_world::graph::{Builder, Slot};
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
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
    Arg::Object(b.graph())
}

fn player_id(h: &Harness) -> u64 {
    h.server.players.values().next().expect("in").character
}

/// The calls a client was sent since `from`: the function, and a use's status when it answers one.
fn calls_to(h: &Harness, b: usize, from: usize) -> Vec<(u32, Option<u32>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let status = match (c.function.0, v.first()) {
                    (USE_DONE, Some(Value::Graph(g))) => g.iter().find_map(|(_, n)| match n {
                        Node::Script { slots, .. } => Some(slots[11].1),
                        _ => None,
                    }),
                    (OPEN_DONE, Some(Value::Int(s))) => Some(*s),
                    _ => None,
                };
                Some((c.function.0, status))
            }
            _ => None,
        })
        .collect()
}

/// A chest a player has open when a reload takes it away: the opener is told it closed, what it
/// held and the chest itself are forgotten, and the player has nothing open.
/// Behaviour: server.operator.reload-lets-go-of-chests-in-use
#[test]
fn a_chest_open_when_a_reload_takes_it_away_is_closed_for_its_opener() {
    let (mut h, b) = hall(
        vec![standing(
            CHEST_ID,
            CHEST,
            CHEST_PACKAGE,
            [60.0, 240.2, -44.6],
        )],
        &["one"],
    );
    h.server.chests.packages.insert(CHEST_PACKAGE);
    let mut chest = Props::default();
    chest.set("GeneratorProfile", Prop::Word(PROFILE));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(CHEST, CHEST_PACKAGE, chest));
    h.server.catalog.insert(yalain_testkit::test_template(
        CRYSTAL,
        0x3A1,
        Props::default(),
    ));
    h.server
        .catalog
        .profiles
        .insert(PROFILE, "GP_TestChestHoard".into());
    h.server.loot.profiles.push(LootProfile {
        name: "GP_TestChestHoard".into(),
        counts: vec![1.0],
        items: Vec::new(),
        extras: vec![(CRYSTAL, 1.0)],
        placed: "authored for a test".into(),
        confidence: "authored".into(),
        chest: None,
    });
    let me = player_id(&h);
    let from = h.bots[b[0]].inbox.len();
    h.bots[b[0]].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(CHEST_ID, me)]),
    );
    h.run(40);
    assert!(
        calls_to(&h, b[0], from).contains(&(OPEN_DONE, Some(0))),
        "opened"
    );
    let held = h.server.chests.by_id[&CHEST_ID].contents.clone();
    assert_eq!(held.len(), 1, "a crystal in it");

    let from = h.bots[b[0]].inbox.len();
    let mut t = tables_of(&h);
    t.content.objects.clear();
    reload(&mut h, t);
    h.run(5);

    assert!(
        calls_to(&h, b[0], from).contains(&(CLOSE_DONE, None)),
        "told it closed"
    );
    let gone: Vec<u64> = forgotten(&h, b[0], from).iter().map(|f| f.0).collect();
    assert!(
        gone.contains(&held[0]) && gone.contains(&CHEST_ID),
        "{gone:?}"
    );
    assert!(h.server.things.get(held[0]).is_none());
    assert!(h.server.chests.by_id.is_empty());
    assert_eq!(
        h.server.players.values().next().expect("in").open_container,
        None
    );
}

/// A door opening for a player's use when a reload moves it: the use is answered, refused as busy;
/// the door is made again closed where it now stands, and nothing more of its opening is told.
/// Behaviour: server.operator.reload-lets-go-of-doors-in-use
#[test]
fn a_door_a_reload_moves_while_it_opens_answers_its_use_and_stands_closed() {
    let (mut h, b) = hall(
        vec![standing(DOOR_ID, DOOR, DOOR_PACKAGE, [60.0, 241.2, -44.6])],
        &["one"],
    );
    h.server
        .doors
        .packages
        .insert(DOOR_PACKAGE, doors::Kind::Plain);
    let me = player_id(&h);
    h.bots[b[0]].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(DOOR_ID, me)]),
    );
    h.run(5);
    assert_eq!(
        h.server.doors.by_id.get(&DOOR_ID).map(|d| d.phase),
        Some(doors::Phase::Opening),
        "opening"
    );

    let from = h.bots[b[0]].inbox.len();
    let mut t = tables_of(&h);
    t.content.objects[0].origin = [61.0, 241.2, -44.6];
    reload(&mut h, t);
    h.run(300);

    let answers: Vec<_> = calls_to(&h, b[0], from)
        .into_iter()
        .filter(|c| c.0 == USE_DONE)
        .collect();
    assert_eq!(
        answers,
        [(USE_DONE, Some(yalain_world::places::ACTIONS_LOCKED))],
        "the use answered, busy"
    );
    let remade: Vec<_> = h
        .world_messages(b[0], from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == DOOR_ID => Some(o),
            _ => None,
        })
        .collect();
    assert_eq!(remade.len(), 1);
    assert_eq!(remade[0].physics.instance_stamp, 2);
    assert_eq!(remade[0].physics.mode, doors::mode::CLOSED);
    assert!(
        h.world_messages(b[0], from).iter().all(|m| !matches!(
            m,
            ServerMessage::DoMode(e) if e.sender.id == DOOR_ID
        )),
        "no more of its opening"
    );
    assert!(!h.server.doors.by_id.contains_key(&DOOR_ID));
}
