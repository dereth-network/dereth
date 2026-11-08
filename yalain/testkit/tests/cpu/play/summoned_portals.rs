//! Summoned portals: Summon Lifestone and Summon Portal open a portal ahead of their summoner, to
//! its lifestone or its last portal's place, which anyone may use until it goes 30 s later.

use auberean_primitives::Vec3;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_store::StoredPosition;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Value};
use yalain_world::graph::{Builder, Slot};
use yalain_world::places::{function as trip, fx, kind, USE_FLAGS_TELEPORTED};
use yalain_world::skill_use::function;
use yalain_world::usable::portals::{SUMMON_LIFESTONE, SUMMON_PORTAL, TEMPLATE};

const SWORD: u32 = 0x4700_051C;
const SHOW_TEXT: u32 = 0x8205_0011;
const VIGOR: u32 = 259;

/// Where the summoner's kept place is: another landblock's.
const ELSEWHERE: StoredPosition = StoredPosition {
    cell: 0x7000_0001,
    origin: [10.0, 10.0, 50.0],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

/// A player in the world with a sword in hand (a summoning needs something in the right hand)
/// and the portal's template known; returns the harness, its client and the player.
fn summoner() -> (Harness, usize, u64) {
    let mut h = Harness::new();
    h.server.catalog.insert(yalain_testkit::test_template(
        TEMPLATE,
        0xF12,
        yalain_content::Props::default(),
    ));
    let (b, me) = h.player_in_world("one", "Elara");
    h.arm(b, me, SWORD);
    h.run(5);
    (h, b, me)
}

/// Keep a place of a kind for the player.
fn keeps(h: &mut Harness, me: u64, k: u32, at: StoredPosition) {
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == me)
        .expect("in the world");
    p.places.retain(|(x, _)| *x != k);
    p.places.push((k, at));
}

fn use_skill(h: &mut Harness, b: usize, skill: u32) -> usize {
    h.all_hold(skill);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(1), Arg::Int(skill), Arg::LongInt(0)],
        ),
    );
    from
}

fn calls_of(h: &Harness, b: usize, from: usize) -> Vec<(u32, Vec<Value>)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some((
                c.function.0,
                c.args
                    .as_ref()
                    .and_then(|a| calls::values(&a.0).ok())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect()
}

/// The text a client was shown since `from`: each line's string and table.
fn texts(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32)> {
    calls_of(h, b, from)
        .into_iter()
        .filter(|(f, _)| *f == SHOW_TEXT)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x| match x.root() {
                Some(calls::Node::TextInfo(t)) => Some((t.string_id, t.table)),
                _ => None,
            })
        })
        .collect()
}

/// The summoned portal a client was told of since `from`: its id and where it stands.
fn portal_made(h: &Harness, b: usize, from: usize) -> Option<(u64, u32, Vec3, [f32; 4])> {
    h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.weenie.entity_did == TEMPLATE => {
            let p = o.physics.position;
            let r = p.frame.rotation;
            Some((o.id, p.cell, p.frame.origin, [r.w, r.x, r.y, r.z]))
        }
        _ => None,
    })
}

/// Has the use ended since `from` (the wait every skill shares begun)?
fn over(h: &Harness, b: usize, from: usize) -> bool {
    h.world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::UpdateTimestamp(q) if q.quality == 303))
}

/// Step until the summoner's story is told; returns the steps taken.
fn until_story(h: &mut Harness, b: usize, from: usize, skill: u32) -> usize {
    for n in 0..400 {
        let told = h
            .world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::DoStory(e) if e.action.skill == skill));
        if told {
            return n;
        }
        h.step();
    }
    panic!("no story");
}

fn position_of(h: &Harness, me: u64) -> StoredPosition {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in the world")
        .position
}

/// Summon Portal powers up as the recall does (10 s held, 12.68 s to its story) and costs half the
/// summoner's vigor; 0.69 s after its story the summoner is told "You summon a portal to your last
/// used portal." and the portal appears 3 m straight ahead of it, at its height, facing its way;
/// the use ends after. The portal goes 30 s after it appeared, not before.
/// Behaviour: server.places.summoned-portals
#[test]
fn summon_portal_opens_a_portal_ahead_of_its_summoner_that_goes_after_thirty_seconds() {
    let (mut h, b, me) = summoner();
    keeps(&mut h, me, kind::LAST_PORTAL, ELSEWHERE);
    let at = position_of(&h, me);
    let from = use_skill(&mut h, b, SUMMON_PORTAL);
    // Its story after the power-up's 12.68 s.
    let steps = until_story(&mut h, b, from, SUMMON_PORTAL);
    assert!((250..=258).contains(&steps), "{steps} steps to the story");
    let vigor: Vec<i32> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == VIGOR => Some(q.value),
            _ => None,
        })
        .collect();
    assert_eq!(vigor, [46], "half of its 100 vigor, and the sword's 4");
    // The portal 0.69 s after the story: not at 0.65 s, there at 0.70 s.
    h.run(13);
    assert!(portal_made(&h, b, from).is_none(), "not before 0.69 s");
    h.run(1);
    let (portal, cell, origin, rotation) = portal_made(&h, b, from).expect("the portal");
    assert_eq!(cell, at.cell);
    let d = [
        origin.x - at.origin[0],
        origin.y - at.origin[1],
        origin.z - at.origin[2],
    ];
    let flat = dereth_primitives::num::math::hypotf(d[0], d[1]);
    assert!((flat - 3.0).abs() < 0.05, "3 m ahead: {flat}");
    assert!(d[2].abs() < 1e-4, "at its height");
    let want = at.rotation;
    let ahead = yalain_world::drops::landing(&at, 3.0);
    assert!(
        (origin.x - ahead.origin[0]).abs() < 1e-3 && (origin.y - ahead.origin[1]).abs() < 1e-3,
        "straight ahead"
    );
    assert!(
        (rotation[0] - want[0]).abs() < 1e-3 && (rotation[3] - want[3]).abs() < 1e-3,
        "facing its way"
    );
    assert_eq!(
        texts(&h, b, from),
        [(0x912E_FF1D, 0x2500_115D)],
        "\"You summon a portal to your last used portal.\""
    );
    // The use's end at 1.59 s: not at 1.50 s, by 1.65 s.
    h.run(16);
    assert!(!over(&h, b, from), "the use not over yet");
    h.run(3);
    assert!(over(&h, b, from), "the use over 0.90 s after the portal");
    let gone = |h: &Harness| {
        h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == portal))
    };
    // Appeared at 0.69 s: still there at 30.65 s, gone by 30.75 s.
    h.run(613 - 33);
    assert!(!gone(&h), "still there before 30 s");
    assert!(h.server.things.get(portal).is_some());
    h.run(2);
    assert!(gone(&h), "gone 30 s after it appeared");
    assert!(h.server.things.get(portal).is_none());
}

/// A use of a portal as the client asks for it.
fn use_of(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
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

/// Another player (not bound anywhere) uses the portal a summoner opened with Summon Lifestone: the
/// portal flashes, the user goes through portal space to the summoner's lifestone place, not its
/// own, and its use is answered with the teleported flags.
/// Behaviour: server.places.summoned-portal-used
#[test]
fn anyone_may_use_a_summoned_portal_to_the_summoners_place() {
    let (mut h, a, me) = summoner();
    keeps(&mut h, me, kind::LIFESTONE, ELSEWHERE);
    let from = use_skill(&mut h, a, SUMMON_LIFESTONE);
    h.run(300);
    assert_eq!(
        texts(&h, a, from),
        [(0x912E_FF1D, 0x2500_115C)],
        "to your lifestone"
    );
    let (portal, ..) = portal_made(&h, a, from).expect("the portal");
    let (b, other) = h.player_in_world("two", "Tysis");
    h.run(10);
    let fb = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            yalain_world::requests::function::USE,
            &[use_of(portal, other)],
        ),
    );
    h.run(10);
    let entered = calls_of(&h, b, fb)
        .iter()
        .any(|(f, _)| *f == trip::ENTER_PORTAL_SPACE);
    assert!(entered, "into portal space");
    let flashed = h.world_messages(b, fb).iter().any(|m| {
        matches!(m, ServerMessage::DoFx(e) if e.sender.id == portal && e.action.fx == fx::PORTAL_USED)
    });
    assert!(flashed, "the portal flashes");
    h.run(40);
    let answers: Vec<(u32, u32)> = calls_of(&h, b, fb)
        .into_iter()
        .filter(|(f, _)| *f == trip::USE_DONE)
        .filter_map(|(_, v)| {
            let Some(Value::Graph(g)) = v.first() else {
                return None;
            };
            g.iter().find_map(|(_, n)| match n {
                calls::Node::Script { slots, .. } => Some((slots[11].1, slots[17].1)),
                _ => None,
            })
        })
        .collect();
    assert_eq!(
        answers,
        [(0, USE_FLAGS_TELEPORTED)],
        "answered as a teleport"
    );
    let there = position_of(&h, other);
    assert_eq!(
        (there.cell, there.origin),
        (ELSEWHERE.cell, ELSEWHERE.origin)
    );
}

/// A summoner keeping no place of the skill's kind (never bound to a lifestone) plays the whole
/// summoning and opens no portal; its use ends as it would.
/// Behaviour: server.places.summoned-portal-nowhere
#[test]
fn a_summoner_never_bound_opens_no_portal() {
    let (mut h, b, _) = summoner();
    let from = use_skill(&mut h, b, SUMMON_LIFESTONE);
    h.run(320);
    assert!(portal_made(&h, b, from).is_none(), "no portal");
    assert!(texts(&h, b, from).is_empty(), "nothing said");
    assert!(over(&h, b, from), "the use over");
    assert!(!h
        .server
        .things
        .by_id
        .values()
        .any(|t| t.template == TEMPLATE));
}
