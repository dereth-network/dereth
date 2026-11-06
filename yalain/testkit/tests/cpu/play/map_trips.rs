//! Maps handed to people who send players where they lead: a cartomancer of Linvak sends the
//! giver of a Linvak map to the place the world names for it, handing the map back; a map of
//! another land, or one whose place the world does not hold, is handed back with its line and no
//! one is sent; a person with no land of its own sends players only with the maps its lines name.
//! Fixture: a cartomancer and a miller beside the start, a Linvak map, an Omishan map and a
//! starter map as made-up templates.

use auberean_protocol::{queue, ServerMessage};
use yalain_content::talk::{How, Line, Role, TalkTable, When, Words};
use yalain_content::{
    ContentObject, ContentPack, Description, Facts, Kind, Place, Prop, Props, Template,
};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{Node, Value};
use yalain_world::gives::{GIVE, HANDED_AFTER, HANDED_BACK_AFTER, HANDING};
use yalain_world::inventory::{move_graph, Move};
use yalain_world::places::{function as trip, IN_PORTAL_SPACE};

const MOVE_DONE: u32 = 0x820E_005E;
const HEAR_TELL: u32 = 0x8205_0017;

const CARTOMANCER: u64 = 0x8130_0000_0000_0801;
const MILLER: u64 = 0x8130_0000_0000_0802;
const CARTOMANCER_T: u32 = 0x4700_2EDD;
const MILLER_T: u32 = 0x4700_2FBC;

/// A Linvak map, an Omishan one, the starter map, and an item that is no map.
const LINVAK_MAP: u32 = 0x4700_0C2A;
const OMISHAN_MAP: u32 = 0x4700_0C4B;
const STARTER_MAP: u32 = 0x4700_1425;
const JUNK: u32 = 0x4700_2108;

/// The people's lines: sending, refusing a map, refusing any other give.
const TELEPORT: u32 = 0xE380_0D93;
const OTHER_LAND: u32 = 0x975F_1228;
const NOT_A_MAP: u32 = 0x098E_5251;

/// Where the Linvak map and the starter map lead.
const LINVAK_PLACE: Place = Place {
    cell: 0x8062_0005,
    origin: [3.8, 98.6, 96.47],
    rotation: [0.817_145_9, 0.0, 0.0, -0.576_432_1],
};
const STARTER_PLACE: Place = Place {
    cell: 0x8FB4_0015,
    origin: [40.4, 81.0, 108.55],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

fn person(id: u64, template: u32, origin: [f32; 3]) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package: 0x103E,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: Some((0xE129_7E08, 0x2500_28AF)),
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
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

fn line(who: u32, string: u32, role: Role, item: Option<u32>) -> Line {
    Line {
        who,
        object: None,
        words: Words::Ref(string, 0x2500_28AF),
        role: Some(role),
        how: How::Tell,
        when: When::Give,
        quest: None,
        update: None,
        phase: None,
        item,
        order: 0,
        measured: true,
        placed: "authored for a test".into(),
    }
}

fn map(id: u32, effect: &str, min: u32, max: u32) -> Template {
    let mut p = Props::default();
    p.set("Usage_Effect2", Prop::Text(effect.into()))
        .set("Usage_RequiredLocationTsysMin", Prop::Place(min, [0.0; 3]))
        .set("Usage_RequiredLocationTsysMax", Prop::Place(max, [0.0; 3]));
    yalain_testkit::test_template(id, 0x1BF8, p)
}

/// The world: a Linvak cartomancer (its table names its land) and a miller (no land; his line
/// names the starter map), the three maps, and the places the Linvak and starter maps lead.
fn world() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let mut c = Props::default();
    c.set(
        "AI_NPCTable",
        Prop::Text("NPCTable_MapTeleporter_Linvak".into()),
    );
    h.server
        .catalog
        .insert(yalain_testkit::test_template(CARTOMANCER_T, 0x13A4, c));
    let mut m = Props::default();
    m.set("AI_NPCTable", Prop::Text("NPCTable_MillerWindmill".into()));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(MILLER_T, 0x13A4, m));
    for t in [
        map(
            LINVAK_MAP,
            "Eff_SummonMine_Granite_Linvak",
            0x7F64_001E,
            0x8161_0003,
        ),
        map(
            OMISHAN_MAP,
            "Eff_SummonMine_Crystal_Omishan",
            0xF47D_0034,
            0xF775_0036,
        ),
        map(
            STARTER_MAP,
            "Eff_SummonMine_Stone_Osteth",
            0x8EB3_0029,
            0x8EB3_0029,
        ),
    ] {
        h.server.catalog.insert(t);
    }
    h.server.content = ContentPack {
        objects: vec![
            person(CARTOMANCER, CARTOMANCER_T, [60.0, 241.0, -44.6]),
            person(MILLER, MILLER_T, [58.0, 241.0, -44.6]),
        ],
        ..ContentPack::default()
    };
    h.server.talk = TalkTable {
        lines: vec![
            line(CARTOMANCER_T, TELEPORT, Role::Teleport, None),
            // A line for a map not taken was never heard (2017 refused no map): the others were.
            Line {
                measured: false,
                ..line(CARTOMANCER_T, OTHER_LAND, Role::RefuseMap, None)
            },
            line(CARTOMANCER_T, NOT_A_MAP, Role::RefuseGive, None),
            line(MILLER_T, TELEPORT, Role::Teleport, Some(STARTER_MAP)),
            Line {
                measured: false,
                ..line(MILLER_T, OTHER_LAND, Role::RefuseMap, None)
            },
        ],
    };
    h.server.places_named.insert(LINVAK_MAP, LINVAK_PLACE);
    h.server.places_named.insert(STARTER_MAP, STARTER_PLACE);
    h
}

/// A player in the world whose client has drawn it.
fn player(h: &mut Harness) -> (usize, u64) {
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    (b, me)
}

fn world_of(h: &Harness, me: u64) -> yalain_net::ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .map(|(c, _)| c)
        .expect("in the world")
}

/// An item of a template in the player's pack; returns it and its slot.
fn hold(h: &mut Harness, me: u64, template: u32) -> (u64, i32) {
    if h.server.catalog.get(template).is_none() {
        h.server.catalog.insert(yalain_testkit::test_template(
            template,
            0x1067,
            Props::default(),
        ));
    }
    let world = world_of(h, me);
    let item =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, template, 1).expect("made");
    h.run(3);
    let slot = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|x| x.id == item)
        .map_or(0, |x| x.slot);
    (item, i32::try_from(slot).unwrap_or(0))
}

/// The client's request to give `item` from `slot` to `target`, as the 2017 clients sent it.
fn give(h: &mut Harness, b: usize, me: u64, item: u64, slot: i32, target: u64) -> usize {
    let m = Move {
        move_type: GIVE,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        target,
        actual_target: target,
        merge_container: target,
        from: me,
        actual_from: me,
        item,
        split_item: item,
        quantity: 1,
        quantity_left: 1,
        from_slot: slot,
        actual_from_slot: slot,
        ..Move::default()
    };
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::drops::function::MOVE_ITEM, &[move_graph(&m)]),
    );
    from
}

/// The moves answered since `from`: who from, to whom, and the status.
fn answers(h: &Harness, b: usize, from: usize) -> Vec<(u64, u64, u32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == MOVE_DONE)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x: &Value| match x.root() {
                Some(Node::Script { slots, .. }) => Some(slots.clone()),
                _ => None,
            })
        })
        .map(|s| {
            let long = |i: usize| u64::from(s[i].1) | u64::from(s[i + 1].1) << 32;
            (long(16), long(2), s[40].1)
        })
        .collect()
}

/// The lines told since `from`.
fn told(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == HEAR_TELL)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some(t.string_id),
                _ => None,
            })
        })
        .collect()
}

fn entered_portal_space(h: &Harness, b: usize, from: usize) -> bool {
    h.calls(b, from)
        .iter()
        .any(|(f, _)| *f == trip::ENTER_PORTAL_SPACE)
}

fn holds(h: &Harness, me: u64, item: u64) -> bool {
    h.server.players[&world_of(h, me)]
        .play
        .items
        .iter()
        .any(|x| x.id == item)
}

fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / yalain_testkit::STEP).ceil() as usize;
    n
}

/// A map of the cartomancer's land is answered as any give; the cartomancer says its line and hands
/// the map back with its handing-over; in the instant it is back (1.30 s later, not before) the
/// giver goes into portal space, and comes out at the place the world names for the map, facing
/// as the place does. The map is kept, and no portal is made.
///
/// Behaviour: server.gives.map-sends-the-giver
#[test]
fn a_cartomancer_given_a_map_of_its_land_sends_the_giver_where_it_leads() {
    let mut h = world();
    let (b, me) = player(&mut h);
    let (item, slot) = hold(&mut h, me, LINVAK_MAP);
    let from = give(&mut h, b, me, item, slot, CARTOMANCER);
    h.run(steps(HANDED_AFTER) + 6);
    assert_eq!(answers(&h, b, from), [(me, CARTOMANCER, 0)], "answered");
    assert_eq!(told(&h, b, from), [TELEPORT], "its line");
    assert!(
        h.world_messages(b, from).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.sender.id == CARTOMANCER
                && e.action.behavior == HANDING && e.action.target == item)),
        "the cartomancer hands the map back"
    );
    assert!(
        !entered_portal_space(&h, b, from),
        "not before the map is back"
    );
    h.run(steps(HANDED_BACK_AFTER));
    assert_eq!(answers(&h, b, from).len(), 2, "the map back");
    assert_eq!(answers(&h, b, from)[1], (CARTOMANCER, me, 0));
    assert!(entered_portal_space(&h, b, from), "into portal space");
    assert!(holds(&h, me, item), "the map kept");
    h.run(steps(IN_PORTAL_SPACE) + 4);
    let at = h.server.players[&world_of(&h, me)].position;
    assert_eq!(
        (at.cell, at.origin, at.rotation),
        (
            LINVAK_PLACE.cell,
            LINVAK_PLACE.origin,
            LINVAK_PLACE.rotation
        )
    );
    assert!(
        !h.server
            .things
            .placed()
            .any(|t| t.kind == yalain_world::things::Kind::Portal),
        "no portal made"
    );
}

/// A map of another land is handed back with the cartomancer's own line for such a map, and no one
/// is sent; an item that is no map gets its refusal of a give.
///
/// Behaviour: server.gives.map-of-another-land
#[test]
fn a_map_of_another_land_is_handed_back_with_the_persons_line_for_it() {
    let mut h = world();
    h.server.places_named.insert(OMISHAN_MAP, LINVAK_PLACE);
    let (b, me) = player(&mut h);
    let start = h.server.players[&world_of(&h, me)].position;
    let (item, slot) = hold(&mut h, me, OMISHAN_MAP);
    let from = give(&mut h, b, me, item, slot, CARTOMANCER);
    h.run(steps(HANDED_AFTER + HANDED_BACK_AFTER + IN_PORTAL_SPACE) + 8);
    assert_eq!(told(&h, b, from), [OTHER_LAND]);
    assert_eq!(answers(&h, b, from)[1], (CARTOMANCER, me, 0), "back");
    assert!(!entered_portal_space(&h, b, from), "no one sent");
    assert_eq!(
        h.server.players[&world_of(&h, me)].position.cell,
        start.cell
    );
    let (junk, slot) = hold(&mut h, me, JUNK);
    let from = give(&mut h, b, me, junk, slot, CARTOMANCER);
    h.run(steps(HANDED_AFTER + HANDED_BACK_AFTER) + 8);
    assert_eq!(told(&h, b, from), [NOT_A_MAP]);
    assert!(holds(&h, me, junk));
}

/// A map of the cartomancer's own land whose place the world does not hold (a world without the
/// maps' places) is refused as one of another land: handed back, no one sent.
///
/// Behaviour: server.gives.map-with-no-place
#[test]
fn a_map_whose_place_the_world_does_not_hold_sends_no_one() {
    let mut h = world();
    h.server.places_named.clear();
    let (b, me) = player(&mut h);
    let start = h.server.players[&world_of(&h, me)].position;
    let (item, slot) = hold(&mut h, me, LINVAK_MAP);
    let from = give(&mut h, b, me, item, slot, CARTOMANCER);
    h.run(steps(HANDED_AFTER + HANDED_BACK_AFTER + IN_PORTAL_SPACE) + 8);
    assert_eq!(told(&h, b, from), [OTHER_LAND]);
    assert_eq!(answers(&h, b, from).len(), 2, "handed back");
    assert!(!entered_portal_space(&h, b, from));
    assert!(holds(&h, me, item));
    let at = h.server.players[&world_of(&h, me)].position;
    assert_eq!((at.cell, at.origin), (start.cell, start.origin));
}

/// A person whose table names no land (the miller) sends players only with the maps its lines
/// name: the starter map to its place, a Linvak map refused with its line.
///
/// Behaviour: server.gives.map-named-by-the-persons-line
#[test]
fn a_person_with_no_land_sends_players_only_with_the_maps_its_lines_name() {
    let mut h = world();
    let (b, me) = player(&mut h);
    let (linvak, slot) = hold(&mut h, me, LINVAK_MAP);
    let from = give(&mut h, b, me, linvak, slot, MILLER);
    h.run(steps(HANDED_AFTER + HANDED_BACK_AFTER + IN_PORTAL_SPACE) + 8);
    assert_eq!(told(&h, b, from), [OTHER_LAND]);
    assert!(!entered_portal_space(&h, b, from));
    let (starter, slot) = hold(&mut h, me, STARTER_MAP);
    let from = give(&mut h, b, me, starter, slot, MILLER);
    h.run(steps(HANDED_AFTER + HANDED_BACK_AFTER + IN_PORTAL_SPACE) + 8);
    assert_eq!(told(&h, b, from), [TELEPORT]);
    assert!(entered_portal_space(&h, b, from));
    let at = h.server.players[&world_of(&h, me)].position;
    assert_eq!(
        (at.cell, at.origin),
        (STARTER_PLACE.cell, STARTER_PLACE.origin)
    );
}
