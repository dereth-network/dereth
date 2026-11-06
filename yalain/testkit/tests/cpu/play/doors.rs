//! Doors: one used opens for everyone who sees it, stays open ten seconds and closes by itself; a
//! use while it opens, or just after a use closed it, is refused; a locked one refuses a plain use
//! from its front, opens from behind, opens to its key (which is used up) and locks again as it
//! closes. Fixture: the test hall with a door beside the start (facing north, the start in front of
//! it or behind it), a second player watching, and keys of made-up templates.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Lock, Prop, Props};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::usable::doors::{self, mode, quality, refusal};

const DOOR: u64 = 0x4002_9900_0000_0071;
const DOOR_TEMPLATE: u32 = 0x4700_0FCE;
const DOOR_PACKAGE: u32 = 0x0EB2;
const KEY: u32 = 0x4700_0F00;
const KEY_ID: u32 = 0x4000_0054;
const USE: u32 = 0x8216_0016;
const USE_DONE: u32 = 0x8216_0021;
const DISCRETE_DESTROY: u32 = 0x820E_0058;

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

/// The test hall with a door a step from where characters start (59.06, 240.2), of a plain kind,
/// facing north.
fn hall(x: f32, y: f32) -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(DOOR),
            template: DOOR_TEMPLATE,
            description: Description {
                name_text: None,
                package: DOOR_PACKAGE,
                look: 0x1F00_0FCE,
                enter_fx: 0,
                scale: None,
                name: Some((0x1C06_0001, 0x2500_0001)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
            origin: [x, y, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    h.server
        .doors
        .packages
        .insert(DOOR_PACKAGE, doors::Kind::Plain);
    let mut key = Props::default();
    key.set("Lock_KeyID", Prop::Word(KEY_ID));
    h.server.catalog.insert(yalain_content::Template {
        facts: yalain_content::Facts {
            name: Some((0x2B0C_0002, 0x2500_0002)),
            ..yalain_content::Facts::default()
        },
        ..yalain_testkit::test_template(KEY, 0x18F, key)
    });
    h
}

/// What a client saw of the door and of its own uses since `from`, in order.
fn seen(h: &Harness, b: usize, from: usize) -> Vec<String> {
    h.world_messages(b, from)
        .iter()
        .filter_map(door_seen)
        .collect()
}

/// A message as a client sees the door move or its own use answered.
fn door_seen(m: &ServerMessage) -> Option<String> {
    match m {
        ServerMessage::UpdateIntVisual(q)
            if q.object.id == DOOR && q.quality == quality::PHYSICS =>
        {
            Some(if q.value == quality::PASSABLE {
                "passable".to_string()
            } else {
                "solid".to_string()
            })
        }
        ServerMessage::DoMode(e) if e.sender.id == DOOR => Some(match e.action.0 {
            mode::OPEN => "opening".to_string(),
            mode::CLOSED => "closing".to_string(),
            other => format!("mode {other:X}"),
        }),
        ServerMessage::UpdateBoolVisual(q) if q.object.id == DOOR && q.quality == quality::OPEN => {
            Some(if q.value == 1 { "open" } else { "shut" }.to_string())
        }
        ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
            let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
            let Some(Value::Graph(g)) = v.first() else {
                return None;
            };
            let status = g.iter().find_map(|(_, n)| match n {
                Node::Script { slots, .. } => Some(slots[11].1),
                _ => None,
            })?;
            let text = g.iter().find_map(|(_, n)| match n {
                Node::TextInfo(t) if t.string_id != 0 => Some(t.string_id),
                _ => None,
            });
            Some(match text {
                Some(t) => format!("done {status} {t:08X}"),
                None => format!("done {status}"),
            })
        }
        ServerMessage::PlayerCall(c) if c.function.0 == DISCRETE_DESTROY => {
            Some("key gone".to_string())
        }
        _ => None,
    }
}

/// The use answers a client was sent since `from`: (status, control flags, effective target).
fn answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, u32)> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => Some((slots[11].1, slots[17].1, slots[12].1)),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// Send a use and run the world `seconds` on; the inbox marks before, of each client.
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

/// A door used turns passable and plays its opening for everyone who sees it; when the opening has
/// played it is open and the use is answered; ten seconds later it turns solid and plays its
/// closing, and is shut when that has played.
/// Behaviour: server.places.door-opens-and-closes-by-itself
#[test]
fn a_door_opens_for_everyone_stays_open_ten_seconds_and_closes_by_itself() {
    let mut h = hall(60.0, 239.0);
    let (a, me) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    send_use(&mut h, a, me, DOOR, 0, 1.0);
    assert_eq!(
        seen(&h, b, fb),
        ["passable", "opening"],
        "the opening begins"
    );
    run_for(&mut h, 0.4);
    assert_eq!(seen(&h, a, fa), ["passable", "opening", "open", "done 0"]);
    run_for(&mut h, 9.5);
    assert_eq!(
        seen(&h, b, fb),
        ["passable", "opening", "open"],
        "still open"
    );
    run_for(&mut h, 1.0);
    assert_eq!(
        seen(&h, b, fb),
        ["passable", "opening", "open", "solid", "closing"]
    );
    run_for(&mut h, 2.6);
    assert_eq!(
        seen(&h, b, fb),
        ["passable", "opening", "open", "solid", "closing", "shut"]
    );
}

/// A door out of reach is walked to, within two meters, before anything moves.
/// Behaviour: server.places.door-walked-to
#[test]
fn a_door_out_of_reach_is_walked_to_within_two_meters_first() {
    let mut h = hall(66.0, 239.0);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, DOOR, 0, 0.5);
    let walk = h.world_messages(a, fa).iter().find_map(|m| match m {
        ServerMessage::MoveTo(o) => Some((o.action.params.flags, o.action.params.desired_distance)),
        _ => None,
    });
    assert_eq!(walk, Some((0x63, doors::REACH)));
    assert!(
        seen(&h, a, fa).is_empty(),
        "nothing moves before it is there"
    );
}

/// A use while the door opens is refused as busy, at once; a use of an open door closes it, the
/// use answered when it is shut; for three seconds after, a use is refused as too soon; then it
/// opens again.
/// Behaviour: server.places.door-refuses-while-moving
#[test]
fn a_door_refuses_a_use_while_it_opens_and_for_three_seconds_after_a_use_closed_it() {
    let mut h = hall(60.0, 239.0);
    let (a, me) = h.player_in_world("one", "Elara");
    let (b, me_b) = h.player_in_world("two", "Babyfare");
    h.run(10);
    send_use(&mut h, a, me, DOOR, 0, 0.3);
    let fb = h.bots[b].inbox.len();
    send_use(&mut h, b, me_b, DOOR, 0, 1.5);
    assert_eq!(
        seen(&h, b, fb),
        [format!("done {}", refusal::BUSY), "open".into()],
        "refused while it opens"
    );
    let fb = h.bots[b].inbox.len();
    send_use(&mut h, b, me_b, DOOR, 0, 2.6);
    assert_eq!(seen(&h, b, fb), ["solid", "closing", "shut", "done 0"]);
    let fb = h.bots[b].inbox.len();
    send_use(&mut h, b, me_b, DOOR, 0, 0.2);
    assert_eq!(seen(&h, b, fb), [format!("done {}", refusal::TOO_SOON)]);
    run_for(&mut h, 0.6);
    let fb = h.bots[b].inbox.len();
    send_use(&mut h, b, me_b, DOOR, 0, 0.2);
    assert_eq!(seen(&h, b, fb), ["passable", "opening"], "three seconds on");
}

/// A locked door refuses a plain use from its front, and a key whose id is not its own as the
/// wrong key; its own key turns, the door unlocks and opens, and as it is open the key is used up
/// and the use answered "Your key fits the lock."; it locks again as it closes.
/// Behaviour: server.places.locked-door-opens-to-its-key
#[test]
fn a_locked_door_refuses_a_plain_use_and_opens_to_its_key_which_is_used_up() {
    const OTHER_KEY: u32 = 0x4700_0F01;
    let mut h = hall(60.0, 239.0);
    let mut other = Props::default();
    other.set("Lock_KeyID", Prop::Word(KEY_ID + 1));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(OTHER_KEY, 0x18F, other));
    h.server
        .locks
        .objects
        .insert(DOOR, Lock { key: Some(KEY_ID) });
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in");
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, KEY, 1);
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, OTHER_KEY, 1);
    h.run(3);
    let held = |h: &Harness, t: u32| {
        h.server.players[&world]
            .play
            .items
            .iter()
            .find(|i| i.template == t)
            .map(|i| i.id)
    };
    let (key, other) = (
        held(&h, KEY).expect("a key"),
        held(&h, OTHER_KEY).expect("another"),
    );
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, DOOR, 0, 0.5);
    assert_eq!(seen(&h, a, fa), [format!("done {}", refusal::LOCKED)]);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, other, DOOR, 0.5);
    assert_eq!(
        seen(&h, a, fa),
        [format!("done {}", refusal::WRONG_KEY)],
        "not its key"
    );
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, DOOR, 1.2);
    assert!(seen(&h, a, fa).is_empty(), "the key turns first");
    run_for(&mut h, 1.6);
    assert_eq!(
        seen(&h, a, fa),
        [
            "passable".to_string(),
            "opening".into(),
            "key gone".into(),
            format!("done 0 {:08X}", doors::text::FITS.0),
            "open".into()
        ]
    );
    assert_eq!(
        answers(&h, a, fa),
        [(0, 317, 0)],
        "an item used on an object: its flags, no effective target"
    );
    assert_eq!(held(&h, KEY), None, "used up");
    assert!(held(&h, OTHER_KEY).is_some());
    run_for(&mut h, 16.0);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, DOOR, 0, 0.3);
    assert_eq!(
        seen(&h, a, fa),
        [format!("done {}", refusal::LOCKED)],
        "locked again as it closed"
    );
}

/// A locked door opens to a plain use from behind it.
/// Behaviour: server.places.locked-door-opens-from-behind
#[test]
fn a_locked_door_opens_to_a_plain_use_from_behind() {
    let mut h = hall(60.0, 241.4);
    h.server
        .locks
        .objects
        .insert(DOOR, Lock { key: Some(KEY_ID) });
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, DOOR, 0, 0.3);
    assert_eq!(seen(&h, a, fa), ["passable", "opening"]);
}

/// A door between a creature's camp and where characters start: across the hall at x = 62, facing
/// east (toward the camp, whose one creature stands at 66, 241), and a creature of `template` that
/// notices a player within 20 m and hardly hurts it.
fn hall_with_a_creature(template: u32, uses_doors: Option<bool>) -> Harness {
    use yalain_content::Prop;
    let mut h = hall(62.0, 241.0);
    h.server.content.objects[0].rotation = [
        std::f32::consts::FRAC_1_SQRT_2,
        0.0,
        0.0,
        -std::f32::consts::FRAC_1_SQRT_2,
    ];
    let mut p = Props::default();
    p.set("GameplayStatistics_Level", Prop::Int(2))
        .set("GameplayStatistics_MaxHealth", Prop::Int(460))
        .set("AICombat_NaturalDamage", Prop::Float(0.1))
        .set("AICombat_NaturalVariance", Prop::Float(0.0))
        .set("AI_PerceptionRadius", Prop::Float(20.0));
    if let Some(u) = uses_doors {
        p.set("AI_CanUseDoors", Prop::Bool(u));
    }
    h.server
        .catalog
        .insert(yalain_testkit::test_template(template, 0x1169, p));
    h.server.spawns.camps.push(yalain_content::Camp {
        id: "beyond the door".into(),
        cell: 0x0299_0109,
        origin: [66.0, 241.0, -44.6],
        radius: 0.0,
        population: 1,
        regen: 600.0,
        templates: vec![(template, 1)],
        spots: vec![(0x0299_0109, [66.0, 241.0, -44.6])],
        placed: "authored for the tests".into(),
        confidence: "authored".into(),
        moves: yalain_content::CampMoves::default(),
    });
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    h
}

const BEYOND: u32 = 0x4700_2230;

/// The client has drawn its first scene: from now on creatures notice it.
fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
}
const DOOR_X: f32 = 62.0;

/// How near the door's plane a creature waiting at it may stand, in meters: about a meter and a
/// half short (2017: a median 1.53 m), less a little for its way's slant.
const HELD_SHORT: f32 = 1.4;

/// The door's moving and the creature's places (how far east of the door's plane), in order.
#[derive(Debug, PartialEq)]
enum Seen {
    Door(String),
    Creature(f32),
}

fn door_and_creature(h: &Harness, b: usize, from: usize) -> Vec<Seen> {
    let creature = h.world_messages(b, 0).iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.weenie.entity_did == BEYOND => Some(o.id),
        _ => None,
    });
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::Position(p) | ServerMessage::PositionCell(p)
                if Some(p.sender.id) == creature =>
            {
                Some(Seen::Creature(p.origin.x - DOOR_X))
            }
            other => door_seen(other).map(Seen::Door),
        })
        .collect()
}

/// The door's moving as the player saw it, alone.
fn door_only(v: &[Seen]) -> Vec<&str> {
    v.iter()
        .filter_map(|x| match x {
            Seen::Door(d) => Some(d.as_str()),
            Seen::Creature(_) => None,
        })
        .collect()
}

/// The creature's places (east of the door's plane) before the door showed `event`, or all of them.
fn creature_before(v: &[Seen], event: &str) -> Vec<f32> {
    v.iter()
        .take_while(|x| !matches!(x, Seen::Door(d) if d == event))
        .filter_map(|x| match x {
            Seen::Creature(e) => Some(*e),
            Seen::Door(_) => None,
        })
        .collect()
}

/// A creature going after a player beyond a closed door comes to a meter and a half from the
/// door's plane and opens it as a use does, unanswered, and stands until it is open; then it goes
/// through. The door closes ten seconds after it opened, as after a use.
/// Behaviour: server.places.creature-opens-a-door-on-its-way
#[test]
fn a_creature_on_its_way_opens_a_closed_door_and_goes_through_once_it_is_open() {
    let mut h = hall_with_a_creature(BEYOND, None);
    let (b, _) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let from = h.bots[b].inbox.len();
    let opened = h.run_until(400, |h| {
        door_only(&door_and_creature(h, b, from)).contains(&"open")
    });
    assert!(opened, "the creature opened the door");
    run_for(&mut h, 2.0);
    let v = door_and_creature(&h, b, from);
    assert_eq!(
        door_only(&v),
        ["passable", "opening", "open"],
        "no use answered"
    );
    let held = creature_before(&v, "opening");
    assert!(!held.is_empty(), "it came toward the door");
    assert!(
        held.iter().all(|x| *x >= HELD_SHORT),
        "held a meter and a half short: {held:?}"
    );
    let waiting = creature_before(&v, "open");
    assert!(
        waiting.iter().all(|x| *x > 0.0),
        "not through before it is open: {waiting:?}"
    );
    let all = creature_before(&v, "none");
    assert!(
        all.iter().any(|x| *x < 0.0),
        "through once it is open: {all:?}"
    );
    run_for(&mut h, 7.5);
    assert_eq!(
        door_only(&door_and_creature(&h, b, from)),
        ["passable", "opening", "open"],
        "still open"
    );
    run_for(&mut h, 0.6);
    assert_eq!(
        door_only(&door_and_creature(&h, b, from)),
        ["passable", "opening", "open", "solid", "closing"],
        "closing ten seconds after it opened"
    );
}

/// A creature whose template says it does not use doors stands at a closed door on its way and
/// never opens it.
/// Behaviour: server.places.creature-that-does-not-use-doors-is-held
#[test]
fn a_creature_that_does_not_use_doors_is_held_at_a_closed_door() {
    let mut h = hall_with_a_creature(BEYOND, Some(false));
    let (b, _) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, b);
    let from = h.bots[b].inbox.len();
    run_for(&mut h, 8.0);
    let v = door_and_creature(&h, b, from);
    assert!(door_only(&v).is_empty(), "the door never moves: {v:?}");
    let places = creature_before(&v, "none");
    assert!(
        places.iter().any(|x| *x < 3.0),
        "it came to the door: {places:?}"
    );
    assert!(
        places.iter().all(|x| *x >= HELD_SHORT),
        "held short of it: {places:?}"
    );
}

/// A locked door holds a creature at its front as it holds a player's plain use; a door whose kind
/// lets creatures through its lock opens to it, unless its template says otherwise.
/// Behaviour: server.places.locked-door-holds-a-creature-at-its-front
#[test]
fn a_locked_door_holds_a_creature_at_its_front_unless_its_kind_lets_creatures_through() {
    let run = |through: bool, template_says: Option<bool>| {
        let mut h = hall_with_a_creature(BEYOND, None);
        h.server
            .locks
            .objects
            .insert(DOOR, Lock { key: Some(KEY_ID) });
        if through {
            h.server.doors.creatures_through_locks.insert(DOOR_PACKAGE);
        }
        if let Some(says) = template_says {
            let mut p = Props::default();
            p.set("Lock_AICanIgnoreLock", Prop::Bool(says));
            h.server.catalog.insert(yalain_testkit::test_template(
                DOOR_TEMPLATE,
                DOOR_PACKAGE,
                p,
            ));
        }
        let (b, _) = h.player_in_world("one", "Elara");
        scene_drawn(&mut h, b);
        let from = h.bots[b].inbox.len();
        run_for(&mut h, 8.0);
        let v = door_and_creature(&h, b, from);
        door_only(&v)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    assert!(run(false, None).is_empty(), "held by the lock");
    assert_eq!(
        run(true, None)[..2],
        ["passable", "opening"],
        "its kind lets creatures through"
    );
    assert!(
        run(true, Some(false)).is_empty(),
        "its template keeps them out"
    );
}

/// A creature going after a player that a locked door holds at its front could not reach that
/// player: the player is counted as one it could not reach this fight (its kill no fair fight).
/// One that opens the door and goes through counts no one, nor does the held one once the door
/// is unlocked and it goes through.
/// Behaviour: server.creatures.could-not-reach
#[test]
fn a_creature_a_locked_door_holds_from_its_player_counts_it_as_one_it_could_not_reach() {
    fn unreached(h: &Harness) -> Vec<u64> {
        h.server
            .things
            .by_id
            .values()
            .find(|t| t.template == BEYOND)
            .and_then(|t| t.creature.as_ref())
            .map(|c| c.unreached.clone())
            .unwrap_or_default()
    }
    let run = |locked: bool| {
        let mut h = hall_with_a_creature(BEYOND, None);
        if locked {
            h.server
                .locks
                .objects
                .insert(DOOR, Lock { key: Some(KEY_ID) });
        }
        let (b, me) = h.player_in_world("one", "Elara");
        scene_drawn(&mut h, b);
        run_for(&mut h, 8.0);
        (h, b, me)
    };
    let (mut h, b, me) = run(true);
    assert_eq!(unreached(&h), vec![me], "held by the lock");
    h.server.locks.objects.remove(&DOOR);
    for t in h.server.things.by_id.values_mut() {
        if let Some(c) = t.creature.as_mut() {
            c.unreached.clear();
        }
    }
    let from = h.bots[b].inbox.len();
    run_for(&mut h, 8.0);
    assert!(
        door_only(&door_and_creature(&h, b, from)).contains(&"open"),
        "unlocked, it opened the door"
    );
    assert!(
        unreached(&h).is_empty(),
        "nothing held it since: {:x?}",
        unreached(&h)
    );
    let (h, _, _) = run(false);
    assert!(
        unreached(&h).is_empty(),
        "it opened the door: {:x?}",
        unreached(&h)
    );
}

/// A use of a door while it closes shuts it at once (it is solid already), and is answered when
/// the closing would have ended.
/// Behaviour: server.places.door-closing-cut-short
#[test]
fn a_use_that_cuts_a_closing_short_is_answered_when_the_closing_would_have_ended() {
    let mut h = hall(60.0, 239.0);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    send_use(&mut h, a, me, DOOR, 0, 2.0);
    let fa = h.bots[a].inbox.len();
    let closing = h.run_until(400, |h| seen(h, a, fa).contains(&"closing".to_string()));
    assert!(closing, "it closes by itself");
    // A use a second into the closing (of 2.47 s).
    run_for(&mut h, 1.0);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, DOOR, 0, 0.2);
    assert_eq!(
        seen(&h, a, fa),
        ["closing", "shut"],
        "shut at once, not answered"
    );
    run_for(&mut h, 0.8);
    assert_eq!(seen(&h, a, fa), ["closing", "shut"], "not answered yet");
    run_for(&mut h, 0.6);
    assert_eq!(
        seen(&h, a, fa),
        ["closing", "shut", "done 0"],
        "answered as the closing would have ended"
    );
}

/// A locked door and a player holding its key, with what it needs to use it.
fn locked_door_and_key() -> (Harness, usize, u64, yalain_net::ConnId, u64) {
    let mut h = hall(60.0, 239.0);
    h.server
        .locks
        .objects
        .insert(DOOR, Lock { key: Some(KEY_ID) });
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in");
    yalain_world::inventory::give(&mut h.server, &mut h.net, world, KEY, 1);
    h.run(3);
    let key = h.server.players[&world]
        .play
        .items
        .iter()
        .find(|i| i.template == KEY)
        .map(|i| i.id)
        .expect("a key");
    (h, a, me, world, key)
}

/// A key's asking to go down while it turns is refused: it turns, and the door opens to it. A key
/// gone from the pack all the same when it has turned is not used up, and the door stays shut and
/// locked, ready to be used again.
/// Behaviour: server.places.a-key-gone-opens-nothing
#[test]
fn a_key_is_not_put_down_while_it_turns_and_one_gone_opens_nothing() {
    use yalain_world::inventory::{move_graph, Move};
    let (mut h, a, me, _, key) = locked_door_and_key();
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, DOOR, 0.05);
    let down = Move {
        move_type: yalain_world::drops::DROP,
        merge: true,
        unlock: true,
        check_take: true,
        all_taken: true,
        grab: true,
        contain: true,
        auto_merge: true,
        from: me,
        actual_from: me,
        item: key,
        split_item: key,
        quantity: 1,
        quantity_left: 1,
        ..Move::default()
    };
    let call = server_call(
        yalain_world::drops::function::MOVE_ITEM,
        &[move_graph(&down)],
    );
    h.bots[a].send(true, queue::WEENIE, &call);
    run_for(&mut h, 4.0);
    let told = seen(&h, a, fa);
    assert!(told.contains(&"key gone".to_string()), "used up: {told:?}");
    assert!(h.server.doors.by_id[&DOOR].unlocked, "it opened");

    let (mut h, a, me, world, key) = locked_door_and_key();
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, DOOR, 0.05);
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .items
        .retain(|i| i.id != key);
    run_for(&mut h, 4.0);
    assert!(h.server.things.get(key).is_some(), "not used up");
    assert_eq!(
        seen(&h, a, fa),
        [format!("done {}", refusal::WRONG_KEY)],
        "nothing opens"
    );
    assert!(!h.server.doors.by_id[&DOOR].unlocked, "still locked");
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, DOOR, 0, 0.5);
    assert_eq!(
        seen(&h, a, fa),
        [format!("done {}", refusal::LOCKED)],
        "and shut, to be tried again"
    );
}

/// A key gone from the pack by the time the door has opened leaves it locked, and it shuts at once.
/// Behaviour: server.places.a-key-gone-opens-nothing
#[test]
fn a_key_gone_as_the_door_opens_shuts_it_again_still_locked() {
    let (mut h, a, me, world, key) = locked_door_and_key();
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, DOOR, 1.5);
    assert!(
        seen(&h, a, fa).contains(&"passable".to_string()),
        "it opens"
    );
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .items
        .retain(|i| i.id != key);
    run_for(&mut h, 3.0);
    let told = seen(&h, a, fa);
    assert!(
        told.contains(&format!("done {}", refusal::WRONG_KEY)),
        "{told:?}"
    );
    assert!(told.contains(&"solid".to_string()), "shuts: {told:?}");
    assert!(!h.server.doors.by_id[&DOOR].unlocked, "still locked");
    assert!(h.server.things.get(key).is_some(), "not used up");
}

/// A key with no name to put in it opens as any, its answer keeping the request's own message: the
/// text can't be written without the name.
/// Behaviour: server.places.a-keys-answer-names-it
#[test]
fn a_key_with_no_name_opens_its_door_with_no_text_about_the_lock() {
    let mut h = hall(60.0, 239.0);
    h.server
        .locks
        .objects
        .insert(DOOR, Lock { key: Some(KEY_ID) });
    h.server
        .catalog
        .templates
        .get_mut(&KEY)
        .expect("made")
        .facts
        .name = None;
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in");
    let key =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, KEY, 1).expect("a key");
    h.run(3);
    let fa = h.bots[a].inbox.len();
    send_use(&mut h, a, me, key, DOOR, 1.2);
    run_for(&mut h, 1.6);
    let told = seen(&h, a, fa);
    assert!(told.contains(&"open".to_string()), "{told:?}");
    assert!(
        !told
            .iter()
            .any(|x| x.contains(&format!("{:08X}", doors::text::FITS.0))),
        "{told:?}"
    );
}
