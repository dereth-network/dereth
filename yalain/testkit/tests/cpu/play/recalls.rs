//! The recalls beside the lifestone's: Portal Recall to the last portal used that may be recalled
//! to, the cities' recalls to their places (and their protection), Allegiance Recall to a hall.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Place};
use yalain_store::StoredPosition;
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::{Harness, ARRAMORA_PLACE, CRAGSTONE_PLACE};
use yalain_world::calls::{self, Node, Value};
use yalain_world::effects::function as effect;
use yalain_world::places::kind;
use yalain_world::requests::function;
use yalain_world::skill_use::{
    function as skill, ALLEGIANCE_RECALL, ARRAMORA_RECALL, CRAGSTONE_RECALL, PORTAL_RECALL,
};

const SWORD: u32 = 0x4700_051C;
const SHOW_TEXT: u32 = 0x8205_0011;
/// "You recall to your last used portal." and its kind, "You have recalled to Cragstone.", "Your
/// allegiance is not bound to an Allegiance Hall." (each a string id in its recall's table).
const RECALLED: u32 = 0x912E_FF1D;
const FAILED: u32 = 0xCF46_E5A6;
const PORTAL_RECALL_TABLE: u32 = 0x2500_115B;
const CRAGSTONE_TABLE: u32 = 0x2500_1239;
const ALLEGIANCE_TABLE: u32 = 0x2500_114F;
/// "You are no longer protected by the portal's magic."
const PORTALS_MAGIC_ENDS: (u32, u32) = (0x6E03_B375, 0x2500_000B);

const FIRST: u64 = 0x4002_9900_0000_0061;
const SECOND: u64 = 0x4070_0000_0000_0062;
const BLUE: u32 = 0x4700_114D;
const PINK: u32 = 0x4700_1154;

/// Where the first portal leads (and the second stands), and where the second leads.
const FIRST_TO: Place = Place {
    cell: 0x7000_0001,
    origin: [10.0, 10.0, 50.0],
    rotation: [1.0, 0.0, 0.0, 0.0],
};
const SECOND_TO: Place = Place {
    cell: 0x7100_0001,
    origin: [20.0, 20.0, 40.0],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

fn portal(
    id: u64,
    template: u32,
    cell: u32,
    origin: [f32; 3],
    to: Place,
    flags: u32,
) -> ContentObject {
    ContentObject {
        object: Some(id),
        template,
        description: Description {
            name_text: None,
            package: 0x1A00,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: Some(to),
        facts: Facts {
            portal: flags,
            ..Facts::default()
        },
        walks: None,
        gate: None,
    }
}

/// The texts a client was shown since `from`: each line's string and table.
fn texts(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(Node::TextInfo(t)) => Some((t.string_id, t.table)),
                _ => None,
            })
        })
        .collect()
}

/// The records added on the client since `from`: each one's timeout and flags.
fn added(h: &Harness, b: usize, from: usize) -> Vec<(f32, u32)> {
    h.calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == effect::ADD)
        .filter_map(|(_, v)| {
            let g = v.iter().find_map(|x| match x {
                Value::Graph(g) => Some(g),
                _ => None,
            })?;
            let Some(Node::Script { slots, .. }) = g.first().map(|(_, n)| n) else {
                return None;
            };
            let w = |i: usize| slots.get(i).map_or(0, |(_, x)| *x);
            Some((f32::from_bits(w(6)), w(20)))
        })
        .collect()
}

/// The place the client's player was put at with a newer teleport stamp since `from`, if any.
fn landed(h: &Harness, b: usize, from: usize, me: u64) -> Option<(u32, [f32; 3], u16)> {
    h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::Position(p) if p.sender.id == me && p.teleport_stamp > 0 => Some((
            p.cell,
            [p.origin.x, p.origin.y, p.origin.z],
            p.teleport_stamp,
        )),
        _ => None,
    })
}

fn use_object(h: &mut Harness, b: usize, me: u64, item: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(item, me)]),
    );
    h.run(60);
    from
}

fn use_skill(h: &mut Harness, b: usize, id: u32) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(skill::USE, &[Arg::Int(1), Arg::Int(id), Arg::LongInt(0)]),
    );
    from
}

fn kept(h: &Harness, me: u64, k: u32) -> Option<StoredPosition> {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .and_then(|p| p.places.iter().find(|(x, _)| *x == k).map(|(_, at)| *at))
}

/// Where the text with this string id comes among the messages since `from`, and where the player's
/// first newer place comes.
fn text_and_place(
    h: &Harness,
    b: usize,
    from: usize,
    me: u64,
    id: u32,
) -> (Option<usize>, Option<usize>) {
    let msgs = h.world_messages(b, from);
    let text = msgs.iter().position(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => c
            .args
            .as_ref()
            .and_then(|a| calls::values(&a.0).ok())
            .unwrap_or_default()
            .iter()
            .any(|v| matches!(v.root(), Some(Node::TextInfo(t)) if t.string_id == id)),
        _ => false,
    });
    let place = msgs.iter().position(
        |m| matches!(m, ServerMessage::Position(p) if p.sender.id == me && p.teleport_stamp > 0),
    );
    (text, place)
}

/// Portal Recall takes a player to where the last portal it used that may be recalled to took it:
/// a portal that may not (a quick exit) leaves the place as it was. The recall powers up as the
/// lifestone's does, then the player comes out there, told "You recall to your last used
/// portal." before its new place.
/// Behaviour: server.places.portal-recall
#[test]
fn portal_recall_returns_to_the_last_portal_used_that_may_be_recalled_to() {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![
            portal(
                FIRST,
                BLUE,
                0x0299_0109,
                [61.0, 240.2, -44.6],
                FIRST_TO,
                0x3,
            ),
            portal(
                SECOND,
                PINK,
                FIRST_TO.cell,
                [11.0, 10.0, 50.0],
                SECOND_TO,
                0,
            ),
        ],
        ..ContentPack::default()
    };
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = use_object(&mut h, b, me, FIRST);
    assert_eq!(landed(&h, b, from, me).map(|l| l.0), Some(FIRST_TO.cell));
    let from = use_object(&mut h, b, me, SECOND);
    assert_eq!(landed(&h, b, from, me).map(|l| l.0), Some(SECOND_TO.cell));
    let link = kept(&h, me, kind::LAST_PORTAL).expect("a place kept");
    assert_eq!(
        (link.cell, link.origin),
        (FIRST_TO.cell, FIRST_TO.origin),
        "the quick exit left the first portal's place"
    );
    h.arm(b, me, SWORD);
    let from = use_skill(&mut h, b, PORTAL_RECALL);
    h.run(400);
    let (cell, at, stamp) = landed(&h, b, from, me).expect("recalled");
    assert_eq!((cell, at, stamp), (FIRST_TO.cell, FIRST_TO.origin, 3));
    assert!(texts(&h, b, from).contains(&(RECALLED, PORTAL_RECALL_TABLE)));
    let (text, place) = text_and_place(&h, b, from, me, RECALLED);
    assert!(
        text < place,
        "the text before the place: {text:?} {place:?}"
    );
}

/// A portal examined says what may be recalled to it, as its flags say: "You can recall and summon
/// this portal.", "You can recall this portal.", "You cannot recall or summon this portal.".
/// Behaviour: server.examination.portal-recall-line
#[test]
fn a_portals_recall_line_is_what_its_flags_allow() {
    use yalain_world::profile::{line, portal_line};
    assert_eq!(portal_line(0x3), line::RECALL_SUMMON_PORTAL);
    assert_eq!(portal_line(0x1), line::RECALL_PORTAL);
    assert_eq!(portal_line(0), line::CANNOT_RECALL_PORTAL);
    assert_eq!(portal_line(0x2), line::CANNOT_RECALL_PORTAL);
}

/// The players' world with a city's places named, and a player with a sword in hand.
fn in_city_world() -> (Harness, usize, u64) {
    let mut h = Harness::new();
    let at = |cell: u32| Place {
        cell,
        origin: [118.55, 114.2, 85.9],
        rotation: [0.39, 0.0, 0.0, -0.92],
    };
    h.server
        .places_by_name
        .insert(CRAGSTONE_PLACE.into(), at(0x6FA9_002E));
    h.server
        .places_by_name
        .insert(ARRAMORA_PLACE.into(), at(0x408E_003F));
    let (b, me) = h.player_in_world("one", "Elara");
    h.run(10);
    h.arm(b, me, SWORD);
    (h, b, me)
}

/// A city's recall powers up as the lifestone's does and takes the player to the city's place,
/// "You have recalled to Cragstone." before its new place. As it ends the player is protected for
/// 30 s (the protection added, its glow played); then "You are no longer protected by the portal's
/// magic." and the protection taken away.
/// Behaviour: server.places.city-recall
#[test]
fn a_city_recall_takes_the_player_to_its_city_and_protects_it_thirty_seconds() {
    let (mut h, b, me) = in_city_world();
    let from = use_skill(&mut h, b, CRAGSTONE_RECALL);
    let protected = |h: &Harness| {
        h.server
            .players
            .values()
            .any(|p| p.character == me && p.protected.is_some())
    };
    assert!(h.run_until(500, |h| protected(h)), "protected as it ends");
    h.run(2);
    let still = h.bots[b].inbox.len();
    let (cell, at, _) = landed(&h, b, from, me).expect("recalled");
    assert_eq!((cell, at), (0x6FA9_002E, [118.55, 114.2, 85.9]));
    assert!(texts(&h, b, from).contains(&(RECALLED, CRAGSTONE_TABLE)));
    let (text, place) = text_and_place(&h, b, from, me, RECALLED);
    assert!(text < place, "the text before the place");
    assert!(
        added(&h, b, from).contains(&(30.0, 0x42)),
        "the protection added: {:?}",
        added(&h, b, from)
    );
    assert!(h.world_messages(b, from).iter().any(|m| {
        matches!(m, ServerMessage::DoFx(e) if e.sender.id == me && e.action.fx == 0x4100_000F)
    }));
    h.run(560);
    assert!(
        !texts(&h, b, still).contains(&PORTALS_MAGIC_ENDS),
        "still protected 28 s on"
    );
    h.run(60);
    assert!(texts(&h, b, still).contains(&PORTALS_MAGIC_ENDS));
    assert!(h.functions(b, still).contains(&effect::REMOVE));
}

/// Arramora's recall takes the player to its place as the cities' do, protected the same, with no
/// word of where it came out (2017: none of its 4).
/// Behaviour: server.places.arramora-recall
#[test]
fn arramoras_recall_takes_the_player_there_without_a_word() {
    let (mut h, b, me) = in_city_world();
    let from = use_skill(&mut h, b, ARRAMORA_RECALL);
    h.run(400);
    let (cell, _, _) = landed(&h, b, from, me).expect("recalled");
    assert_eq!(cell, 0x408E_003F);
    assert!(!texts(&h, b, from).iter().any(|(id, _)| *id == RECALLED));
    let p = h.server.players.values().find(|p| p.character == me);
    assert!(p.is_some_and(|p| p.protected.is_some()), "protected");
}

/// Allegiance Recall with no hall its allegiance is bound to plays it all and says "Your allegiance
/// is not bound to an Allegiance Hall."; the player stays where it is.
/// Behaviour: server.places.allegiance-recall-unbound
#[test]
fn allegiance_recall_with_no_hall_bound_fails_and_says_so() {
    let (mut h, b, me) = in_city_world();
    let from = use_skill(&mut h, b, ALLEGIANCE_RECALL);
    h.run(400);
    assert_eq!(landed(&h, b, from, me), None);
    assert!(texts(&h, b, from).contains(&(FAILED, ALLEGIANCE_TABLE)));
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::UpdateTimestamp(q) if q.quality == 303)),
        "it ends as a recall does"
    );
}

/// A city's recall whose place the world lacks plays it all and ends without a word, the player
/// where it was and not protected.
/// Behaviour: server.places.city-recall-nowhere
#[test]
fn a_city_recall_with_no_place_ends_without_a_word_or_protection() {
    let (mut h, b, me) = in_city_world();
    h.server.places_by_name.clear();
    let from = use_skill(&mut h, b, CRAGSTONE_RECALL);
    h.run(400);
    assert_eq!(landed(&h, b, from, me), None);
    assert!(!texts(&h, b, from)
        .iter()
        .any(|(_, t)| *t == CRAGSTONE_TABLE));
    assert!(added(&h, b, from).is_empty(), "nothing put on");
    let p = h.server.players.values().find(|p| p.character == me);
    assert!(p.is_some_and(|p| p.protected.is_none()));
    assert!(
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::UpdateTimestamp(q) if q.quality == 303)),
        "it ends as a recall does"
    );
}
