//! Locked portal doors: a portal that stands as a door, locked, carries a player through only on
//! its key: the key's use plays, the door flashes and the player goes through portal space to where
//! it leads, the key used up there unless its kind keeps it and what the door's kind names for its
//! use moving the player's quests; used bare it is locked from either side; another key is the
//! wrong one; a player its quest keeps out is refused at once; its trips wait three seconds between
//! them, not a vault's five. A locked door of a door's kind that leads somewhere is one too.
//! Fixture: the test hall with a locked portal door beside the start that the world numbers (no id
//! of its own), leading elsewhere in the hall; its key, a key whose kind keeps it, and a key of
//! another lock.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::quests::QuestDef;
use yalain_content::{
    ContentObject, ContentPack, Description, Facts, Kind, Lock, Place, Prop, Props,
};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::usable::{doors, vaults};

const DOOR_TEMPLATE: u32 = 0x4700_7701;
const DOOR_PACKAGE: u32 = 0x7701;
const CELL: u32 = 0x0299_0109;
const AT: [f32; 3] = [60.0, 239.0, -44.6];
const AWAY: [f32; 3] = [20.0, 200.0, -44.6];
const KEY: u32 = 0x4700_7702;
const KEPT_KEY: u32 = 0x4700_7703;
const OTHER_KEY: u32 = 0x4700_7704;
const LOCK: u32 = 0x4000_0090;
const QUEST: u32 = 0x8000_7705;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const DISCRETE_DESTROY: u32 = 0x820E_0058;
const USES_ON: u32 = 0x4100_00FC;
const FLASH: u32 = 0x4100_0044;
const FITS: u32 = 0x2317_692F;

/// A use of an item (on an object, or on nothing), as the client asks for it.
fn use_of(item: u64, me: u64, on: u64) -> Arg {
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
            Slot::Long(on),
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

/// The door's kind: a portal door, which names an advance of the test quest for its use and may
/// keep players out of it.
fn door_kind(keeps_out: bool) -> Props {
    let mut p = Props::default();
    p.set(
        "Usage_Effect1",
        Prop::Text("Eff_AdvanceQuestTestDoorPhase2".into()),
    );
    if keeps_out {
        p.set("Usage_RequiredQuest", Prop::Word(QUEST))
            .set("Usage_RequiredQuestStatus", Prop::Word(0x1000_0000));
    }
    p
}

/// The test hall with a locked portal door a step from the start, facing `rotation`, leading
/// elsewhere in the hall; the world numbers it, and knows its lock by where it stands.
fn hall(rotation: [f32; 4], keeps_out: bool) -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: None,
            template: DOOR_TEMPLATE,
            description: Description {
                name_text: None,
                package: DOOR_PACKAGE,
                look: 0x1F00_0ED5,
                enter_fx: 0,
                scale: None,
                name: Some((0x3F7C_29E0, 0x2500_1B40)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: CELL,
            origin: AT,
            rotation,
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: Some(Place {
                cell: CELL,
                origin: AWAY,
                rotation: [1.0, 0.0, 0.0, 0.0],
            }),
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    h.server
        .locks
        .placed
        .insert((CELL, DOOR_TEMPLATE), Lock { key: Some(LOCK) });
    h.server.portal_door_packages.insert(DOOR_PACKAGE);
    h.server.catalog.insert(yalain_testkit::test_template(
        DOOR_TEMPLATE,
        DOOR_PACKAGE,
        door_kind(keeps_out),
    ));
    for (t, key, kept) in [
        (KEY, LOCK, false),
        (KEPT_KEY, LOCK, true),
        (OTHER_KEY, 0x4000_0091, false),
    ] {
        let mut props = Props::default();
        props.set("Lock_KeyID", Prop::Word(key));
        if kept {
            props.set("Item_IsDestroyOnUse", Prop::Bool(false));
        }
        h.server.catalog.insert(yalain_content::Template {
            facts: Facts {
                name: Some((0x2B0C_0002, 0x2500_0002)),
                ..Facts::default()
            },
            ..yalain_testkit::test_template(t, 0x18F, props)
        });
    }
    h.server.quests.quests.push(QuestDef {
        id: QUEST,
        name: yalain_content::Words::Ref(0, 0),
        description: (0, 0),
        icon: 0,
        play_fx: false,
        journals: vec![(1, 0), (2, 0), (3, 0)],
        challenge: 0,
        duration: 0.0,
        timed_phase: 0,
        retry: 0.0,
        xp: 0,
        cancellable: false,
        placed: "authored for a test".into(),
        hidden: false,
        effects: Vec::new(),
        subquests: Vec::new(),
        min_level: 0,
        fails_on_death: false,
        once: false,
        fellows: yalain_content::quests::Fellows::default(),
        ranges: Vec::new(),
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
        cancel_wait: None,
    });
    h.server.quest_ids.insert("QuestTestDoor".into(), QUEST);
    h
}

/// The door's id: the world numbers it.
fn door(h: &Harness) -> u64 {
    yalain_world::presence::content_id(&h.server.content.objects, 0)
}

/// A player in the hall holding a key of a template, its quest just under way when asked:
/// its client, id, connection and key.
fn with_key(
    h: &mut Harness,
    name: &str,
    key: u32,
    on_the_quest: bool,
) -> (usize, u64, yalain_net::ConnId, u64) {
    let (b, me) = h.player_in_world(name, name);
    h.run(10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in")
        .0;
    if on_the_quest {
        h.server
            .players
            .get_mut(&world)
            .expect("in")
            .play
            .quests
            .push(yalain_world::state::QuestState {
                quest: QUEST,
                status: yalain_world::quests::status::UNDERWAY,
                phase: 1,
                started: 0,
                updated: 0,
            });
    }
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, key, 1);
    h.run(3);
    let item = held(h, world, key).expect("a key");
    (b, me, world, item)
}

fn held(h: &Harness, world: yalain_net::ConnId, template: u32) -> Option<u64> {
    h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == template)
        .map(|i| i.id)
}

/// The use answers a client was sent since `from`: (status, control flags, its text).
fn answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                let (status, flags) = g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some((slots[11].1, slots[17].1)),
                    _ => None,
                })?;
                let text = g
                    .iter()
                    .find_map(|(_, n)| match n {
                        Node::TextInfo(t) => Some(t.string_id),
                        _ => None,
                    })
                    .unwrap_or(0);
                Some((status, flags, text))
            }
            _ => None,
        })
        .collect()
}

fn flashed(h: &Harness, b: usize, from: usize) -> bool {
    let id = door(h);
    h.world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::DoFx(e) if e.sender.id == id && e.action.fx == FLASH))
}

fn send_use(h: &mut Harness, b: usize, me: u64, item: u64, on: u64, seconds: f64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(USE, &[use_of(item, me, on)]),
    );
    run_for(h, seconds);
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

fn at_the_far_end(h: &Harness, world: yalain_net::ConnId) -> bool {
    let at = h.server.players[&world].position.origin;
    (at[0] - AWAY[0]).abs() < 0.01 && (at[1] - AWAY[1]).abs() < 0.01
}

fn phase(h: &Harness, world: yalain_net::ConnId) -> u32 {
    yalain_world::quests::phase_of(&h.server.players[&world].play.quests, QUEST)
}

/// The door's key used on it: the player plays the use; when it has played the door flashes and
/// the player goes through portal space to where the door leads, and on arrival the key is used up
/// and the use answered "Your key fits the lock.".
/// Behaviour: server.places.locked-portal-door-carries-its-keys-user
#[test]
fn a_locked_portal_doors_key_carries_its_user_through_and_is_used_up() {
    let mut h = hall([1.0, 0.0, 0.0, 0.0], false);
    let (a, me, world, key) = with_key(&mut h, "one", KEY, true);
    let id = door(&h);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, id, 1.0);
    assert!(
        h.world_messages(a, fa).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == USES_ON)),
        "the key's use plays"
    );
    assert!(answers(&h, a, fa).is_empty(), "nothing answered yet");
    run_for(&mut h, vaults::OPENS_AFTER);
    assert!(flashed(&h, a, fa), "the door flashes as it opens");
    run_for(&mut h, 2.0);
    assert_eq!(answers(&h, a, fa), [(0, 317, FITS)], "answered on arrival");
    assert!(
        h.world_messages(a, fa).iter().any(|m| matches!(m,
            ServerMessage::PlayerCall(c) if c.function.0 == DISCRETE_DESTROY)),
        "the key destroyed"
    );
    assert_eq!(held(&h, world, KEY), None, "used up");
    assert!(at_the_far_end(&h, world), "where the door leads");
}

/// A key whose kind says a use does not use it up carries its user through the door and is kept.
/// Behaviour: server.places.locked-portal-door-carries-its-keys-user
#[test]
fn a_key_whose_kind_keeps_it_carries_its_user_through_and_is_kept() {
    let mut h = hall([1.0, 0.0, 0.0, 0.0], false);
    let (a, me, world, key) = with_key(&mut h, "one", KEPT_KEY, true);
    let id = door(&h);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, id, vaults::OPENS_AFTER + 3.0);
    assert_eq!(answers(&h, a, fa), [(0, 317, FITS)]);
    assert!(at_the_far_end(&h, world), "where the door leads");
    assert_eq!(held(&h, world, KEPT_KEY), Some(key), "kept");
}

/// A locked portal door used bare is locked from either side: refused at once as a locked item,
/// with no flash, and the player not moved.
/// Behaviour: server.places.locked-portal-door-refuses-a-plain-use
#[test]
fn a_locked_portal_door_used_bare_is_locked_from_either_side() {
    for facing in [[1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]] {
        let mut h = hall(facing, false);
        let (a, me, world, _) = with_key(&mut h, "one", KEY, true);
        let id = door(&h);
        let fa = h.bots[a].inbox.len();
        send_use(&mut h, a, me, id, 0, 3.0);
        assert_eq!(
            answers(&h, a, fa),
            [(doors::refusal::LOCKED, doors::USE_FLAGS, 0)],
            "locked, facing {facing:?}"
        );
        assert!(!flashed(&h, a, fa), "no flash");
        assert!(h.server.players[&world].trip.is_none(), "not taken");
        assert!(!at_the_far_end(&h, world));
    }
}

/// A key of another lock used on a locked portal door is the wrong key: refused, kept, and the
/// player not moved.
/// Behaviour: server.places.locked-portal-door-refuses-a-plain-use
#[test]
fn another_key_on_a_locked_portal_door_is_the_wrong_one() {
    let mut h = hall([1.0, 0.0, 0.0, 0.0], false);
    let (a, me, world, key) = with_key(&mut h, "one", OTHER_KEY, true);
    let id = door(&h);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, id, 4.0);
    let told: Vec<u32> = answers(&h, a, fa).iter().map(|a| a.0).collect();
    assert_eq!(told, [doors::refusal::WRONG_KEY]);
    assert!(!flashed(&h, a, fa));
    assert_eq!(held(&h, world, OTHER_KEY), Some(key), "kept");
    assert!(!at_the_far_end(&h, world));
}

/// What the door's kind names for its use moves its user's quest as the key carries it through.
/// Behaviour: server.places.locked-portal-door-moves-its-users-quests
#[test]
fn a_locked_portal_doors_kind_moves_its_users_quest_on_the_trip() {
    let mut h = hall([1.0, 0.0, 0.0, 0.0], false);
    let (a, me, world, key) = with_key(&mut h, "one", KEY, true);
    let id = door(&h);
    assert_eq!(phase(&h, world), 1);
    send_use(&mut h, a, me, key, id, vaults::OPENS_AFTER + 3.0);
    assert!(at_the_far_end(&h, world));
    assert_eq!(phase(&h, world), 2, "advanced by the door's use");
}

/// A locked portal door whose kind keeps out a player without its quest refuses that player's key
/// at once with the quest's refusal: no use plays, the key is kept and the player not moved.
/// Behaviour: server.places.locked-portal-door-kept-by-its-quest
#[test]
fn a_locked_portal_door_its_quest_keeps_out_refuses_its_key_at_once() {
    let mut h = hall([1.0, 0.0, 0.0, 0.0], true);
    let (a, me, world, key) = with_key(&mut h, "one", KEY, false);
    let id = door(&h);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, id, 0.5);
    let told = answers(&h, a, fa);
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].1, yalain_world::places::USE_FLAGS_QUEST_KEEPS_OUT);
    assert!(
        !h.world_messages(a, fa).iter().any(|m| matches!(m,
            ServerMessage::PlayBehavior(e) if e.action.behavior == USES_ON)),
        "no use plays"
    );
    run_for(&mut h, vaults::OPENS_AFTER + 3.0);
    assert_eq!(held(&h, world, KEY), Some(key), "kept");
    assert!(!at_the_far_end(&h, world));
    // On its quest, the same key carries the player through.
    let mut h = hall([1.0, 0.0, 0.0, 0.0], true);
    let (a, me, world, key) = with_key(&mut h, "one", KEY, true);
    let id = door(&h);
    send_use(&mut h, a, me, key, id, vaults::OPENS_AFTER + 3.0);
    assert!(at_the_far_end(&h, world));
}

/// A second player's key, its trip opening four and a half seconds after the door's last, goes
/// through: a locked portal door waits three seconds between trips, not a vault's five.
/// Behaviour: server.places.locked-portal-door-carries-its-keys-user
#[test]
fn a_locked_portal_door_lets_a_trip_through_under_five_seconds_after_its_last() {
    let mut h = hall([1.0, 0.0, 0.0, 0.0], false);
    let (a, me, _, key) = with_key(&mut h, "one", KEY, true);
    let (b, you, world_b, yours) = with_key(&mut h, "two", KEY, true);
    let id = door(&h);
    send_use(&mut h, a, me, key, id, 0.5);
    run_for(&mut h, 4.0);
    assert!(h
        .server
        .players
        .values()
        .any(|p| p.character == me && p.trip.is_none()));
    let fb = h.bots[b].inbox.len();
    send_use(&mut h, b, you, yours, id, vaults::OPENS_AFTER + 3.0);
    assert_eq!(answers(&h, b, fb), [(0, 317, FITS)]);
    assert!(at_the_far_end(&h, world_b));
}

/// A locked door of a door's kind that leads somewhere is a portal door too: used bare it is
/// locked, not a vault wanting its glyph, and its key carries its user through.
/// Behaviour: server.places.locked-portal-door-refuses-a-plain-use
#[test]
fn a_locked_door_of_a_doors_kind_that_leads_somewhere_is_a_portal_door() {
    let mut h = hall([1.0, 0.0, 0.0, 0.0], false);
    h.server.portal_door_packages.clear();
    h.server
        .doors
        .packages
        .insert(DOOR_PACKAGE, doors::Kind::Plain);
    let (a, me, world, key) = with_key(&mut h, "one", KEY, true);
    let id = door(&h);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, id, 0, 0.5);
    assert_eq!(
        answers(&h, a, fa),
        [(doors::refusal::LOCKED, doors::USE_FLAGS, 0)]
    );
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, id, vaults::OPENS_AFTER + 3.0);
    assert_eq!(answers(&h, a, fa), [(0, 317, FITS)]);
    assert!(at_the_far_end(&h, world));
}
