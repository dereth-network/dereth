//! Places: a portal takes a player through portal space to its destination (unless its quest keeps
//! the player out), its own effects put on as the player comes out; a lifestone binds a player, and
//! the recall takes it back where it stood as it bound.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Place};
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::graph::{Builder, Slot};
use yalain_world::places::{function as trip, fx};
use yalain_world::requests::function;
use yalain_world::session::function::{MOVEMENT_FROZEN, SHOW_TEXT};
use yalain_world::skill_use::LIFESTONE_RECALL;

const PORTAL: u64 = 0x4002_9900_0000_0056;
const STONE: u64 = 0x4002_9900_0000_0057;

fn object(id: u64, template: u32, origin: [f32; 3], destination: Option<Place>) -> ContentObject {
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
        cell: 0x0299_0109,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination,
        facts: Facts::default(),
        walks: None,
        gate: None,
    }
}

/// A use of an object as the client asks for it: a use description naming the object and the
/// player.
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

fn calls(h: &Harness, b: usize, from: usize) -> Vec<u32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) => Some(c.function.0),
            _ => None,
        })
        .collect()
}

fn teleported_to(h: &Harness, b: usize, from: usize, id: u64) -> Option<(u32, u16)> {
    h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::Position(p) if p.sender.id == id && p.teleport_stamp > 0 => {
            Some((p.cell, p.teleport_stamp))
        }
        _ => None,
    })
}

/// Using a portal in reach flashes the portal and puts the player in portal space (teleporting
/// text, effects, animation and movement frozen); a second and a half later it leaves portal
/// space, the use is answered, and it is placed at the destination with a newer teleport stamp;
/// a player that saw it at the old place sees it leave.
/// Behaviour: server.places.portal-trip
#[test]
fn a_portal_takes_the_player_through_portal_space_to_its_destination() {
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = ContentPack {
        objects: vec![object(PORTAL, 0x4700_1154, [61.0, 240.2, -44.6], Some(to))],
        ..ContentPack::default()
    };
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let (fa, fb) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, ida)]),
    );
    h.run(10);
    let entering = calls(&h, a, fa);
    assert_eq!(
        entering,
        [
            SHOW_TEXT,
            trip::ANIMATION_FROZEN,
            MOVEMENT_FROZEN,
            trip::ENTER_PORTAL_SPACE
        ],
        "into portal space"
    );
    assert!(h.world_messages(b, fb).iter().any(|m| {
        matches!(m, ServerMessage::DoFx(e) if e.sender.id == PORTAL && e.action.fx == fx::PORTAL_USED)
    }));
    assert_eq!(teleported_to(&h, a, fa, ida), None, "not yet");
    h.run(40);
    let calls_now = calls(&h, a, fa);
    assert!(calls_now.ends_with(&[
        trip::EXIT_PORTAL_SPACE,
        trip::ANIMATION_FROZEN,
        trip::USE_DONE
    ]));
    assert_eq!(teleported_to(&h, a, fa, ida), Some((0x7000_0001, 1)));
    assert!(
        h.world_messages(b, fb)
            .iter()
            .any(|m| matches!(m, ServerMessage::ObjectLeaves(e) if e.sender.id == ida)),
        "the other player sees it leave"
    );
}

/// Using a lifestone binds the player where it stands: the lifestone and the player play their
/// binding and the use is answered. The lifestone recall (a weapon in hand) then powers up for its
/// time, tells its story, and takes the player through portal space to the place and heading it
/// had as it bound, with a newer teleport stamp; its end gives the skill's information and starts
/// the wait every skill shares.
/// Behaviour: server.places.lifestone-bind-and-recall
#[test]
fn a_lifestone_binds_and_the_recall_takes_the_player_back_where_it_bound() {
    let mut h = Harness::new();
    h.server.lifestone_packages.insert(0x1A00);
    h.server.content = ContentPack {
        objects: vec![object(STONE, 0x4700_113A, [60.0, 241.0, -44.6], None)],
        ..ContentPack::default()
    };
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let bound_at = h
        .server
        .players
        .values()
        .next()
        .expect("the player")
        .position;
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(STONE, ida)]),
    );
    h.run(10);
    assert!(h
        .world_messages(a, from)
        .iter()
        .any(|m| { matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == STONE) }));
    assert_eq!(calls(&h, a, from), [trip::USE_DONE], "bound");
    h.arm(a, ida, 0x4700_051C);
    // Walk away, then recall.
    h.bots[a].send(
        true,
        queue::WEENIE,
        &yalain_testkit::play::report(
            0x0299_0109,
            auberean_primitives::Vec3::new(40.0, 200.0, -44.6),
            0,
            yalain_world::world::IDLE_MOTION,
            0,
        ),
    );
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(
            function::ATTACK,
            &[Arg::Int(1), Arg::Int(LIFESTONE_RECALL), Arg::LongInt(0)],
        ),
    );
    h.run(200);
    assert_eq!(
        teleported_to(&h, a, from, ida),
        None,
        "ten seconds on, the recall still powers up"
    );
    h.run(200);
    let landed = h
        .world_messages(a, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::Position(p) if p.sender.id == ida && p.teleport_stamp == 1 => Some(*p),
            _ => None,
        })
        .expect("recalled");
    let (dx, dy) = (
        landed.origin.x - bound_at.origin[0],
        landed.origin.y - bound_at.origin[1],
    );
    assert!(
        (dx * dx + dy * dy).sqrt() < 0.01,
        "where it stood as it bound: {landed:?}"
    );
    h.run(20);
    let after = calls(&h, a, from);
    assert!(
        after.contains(&0x8214_000D),
        "the skill's information: {after:x?}"
    );
    assert!(
        h.world_messages(a, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::UpdateTimestamp(q) if q.quality == 303)),
        "the wait every skill shares begins"
    );
}

/// A use asked for while the player's earlier action is still carried out (here, in portal space)
/// is refused at once: its answer alone, with the status that actions are locked.
/// Behaviour: server.places.actions-locked
#[test]
fn a_use_in_portal_space_is_refused_as_actions_locked() {
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = ContentPack {
        objects: vec![object(PORTAL, 0x4700_1154, [61.0, 240.2, -44.6], Some(to))],
        ..ContentPack::default()
    };
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, ida)]),
    );
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, ida)]),
    );
    h.run(5);
    let answers: Vec<u32> = h
        .world_messages(a, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == trip::USE_DONE => {
                let v = yalain_world::calls::values(&c.args.as_ref()?.0).ok()?;
                let yalain_world::calls::Value::Graph(g) = v.first()? else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    yalain_world::calls::Node::Script {
                        package: 0x245,
                        slots,
                    } => slots.get(11).map(|(_, s)| *s),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();
    assert_eq!(answers, [1029], "refused: actions locked");
}

/// Every kind of lifestone binds: the Lugian's, the Tumerok's, the plain one and any other of the
/// kind are answered as the human's is, the use made and the player told it is bound.
/// Behaviour: server.places.every-lifestone-binds
#[test]
fn every_kind_of_lifestone_binds_the_player() {
    // The human's, the Lugian's, the Tumerok's, the plain one, another of the human's.
    for template in [
        0x4700_113A,
        0x4700_113D,
        0x4700_1143,
        0x4700_113B,
        0x4700_1138,
    ] {
        let mut h = Harness::new();
        h.server.lifestone_packages.insert(0x1A00);
        h.server.content = ContentPack {
            objects: vec![object(STONE, template, [60.0, 241.0, -44.6], None)],
            ..ContentPack::default()
        };
        let (a, ida) = h.player_in_world("one", "Elara");
        h.run(10);
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::USE, &[use_of(STONE, ida)]),
        );
        h.run(10);
        let answers: Vec<u32> = h
            .world_messages(a, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::PlayerCall(c) if c.function.0 == trip::USE_DONE => {
                    let v = yalain_world::calls::values(&c.args.as_ref()?.0).ok()?;
                    let yalain_world::calls::Value::Graph(g) = v.first()? else {
                        return None;
                    };
                    g.iter().find_map(|(_, n)| match n {
                        yalain_world::calls::Node::Script {
                            package: 0x245,
                            slots,
                        } => slots.get(11).map(|(_, s)| *s),
                        _ => None,
                    })
                }
                _ => None,
            })
            .collect();
        assert_eq!(answers, [0], "the use of lifestone {template:08X} is made");
        assert!(
            h.world_messages(a, from)
                .iter()
                .any(|m| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == ida)),
            "the player binds at lifestone {template:08X}"
        );
    }
}

/// Binding to a lifestone plays the player's binding behavior for a moment: it is stopped about a
/// second later, not in the tick it began (so the client shows it).
/// Behaviour: server.places.bind-plays-its-behavior
#[test]
fn binding_plays_the_players_behavior_for_about_a_second() {
    let mut h = Harness::new();
    h.server.lifestone_packages.insert(0x1A00);
    h.server.content = ContentPack {
        objects: vec![object(STONE, 0x4700_113A, [60.0, 241.0, -44.6], None)],
        ..ContentPack::default()
    };
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(STONE, ida)]),
    );
    let stopped = |h: &Harness| {
        h.world_messages(a, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::StopBehavior(e) if e.sender.id == ida))
    };
    h.run(10);
    assert!(
        h.world_messages(a, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == ida)),
        "the player binds"
    );
    assert!(!stopped(&h), "still playing half a second on");
    h.run(30);
    assert!(stopped(&h), "stopped after about a second");
}

/// A trip through portal space ends the player's attack run: out of it, the player is no longer
/// attacking and is at peace, and nothing more is swung at (or told too far from) the old target.
/// Behaviour: server.combat.portal-space-ends-a-run
#[test]
fn a_trip_through_portal_space_ends_an_attack_run() {
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = ContentPack {
        objects: vec![object(PORTAL, 0x4700_1154, [61.0, 240.2, -44.6], Some(to))],
        ..ContentPack::default()
    };
    h.add_creatures(1);
    let (a, ida) = h.player_in_world("one", "Elara");
    h.bots[a].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(10);
    h.arm(a, ida, 0x4700_051C);
    let beetle = h
        .world_messages(a, 0)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o)
                if o.weenie.entity_did == yalain_testkit::TEST_CREATURE =>
            {
                Some(o.id)
            }
            _ => None,
        })
        .expect("the camp's creature");
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(0x8204_0008, &[Arg::LongInt(beetle)]),
    );
    h.run(20);
    assert!(
        h.server.players.values().any(|p| p.attacking.is_some()),
        "attacking"
    );
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, ida)]),
    );
    h.run(60);
    assert!(teleported_to(&h, a, 0, ida).is_some(), "through the portal");
    assert!(
        h.server.players.values().all(|p| p.attacking.is_none()),
        "the run is over"
    );
    let after = h.bots[a].inbox.len();
    h.run(60);
    assert!(
        !calls(&h, a, after).contains(&0x8204_001C),
        "nothing told too far after: {:x?}",
        calls(&h, a, after)
    );
}

/// Walking into a portal of a kind used so takes the player as a use does, with no request: into
/// portal space as the bodies come to touch, and the use answered as a teleport's from where it
/// stood; staying in touch does not take it again, and a portal of another kind (a summoned one)
/// is walked through. The bodies: the portal's sphere of 0.878 m 0.841 m up, the player's of
/// 0.555 m at 0.55 m and 1.50 m.
/// Behaviour: server.places.walked-into-portal
#[test]
fn walking_into_a_portal_takes_the_player_as_a_use_does_and_once_while_touching() {
    use auberean_primitives::Vec3;
    use yalain_testkit::play::report;
    use yalain_world::world::IDLE_MOTION;
    const WALKED: u64 = 0x4002_9900_0000_0058;
    const SUMMONED: u64 = 0x4002_9900_0000_0059;
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let mut walked = object(WALKED, 0x4700_1154, [70.0, 240.0, -44.6], Some(to));
    walked.description.package = 0x1A01;
    let summoned = object(SUMMONED, 0x4700_0604, [80.0, 240.0, -44.6], Some(to));
    h.server.content = ContentPack {
        objects: vec![walked, summoned],
        ..ContentPack::default()
    };
    h.server.walk_in_packages.insert(0x1A01);
    let (a, _) = h.player_in_world("one", "Elara");
    h.run(10);
    let cell = h.server.players.values().next().expect("in").position.cell;
    let at = |x: f32| report(cell, Vec3::new(x, 240.0, -44.6), 0, IDLE_MOTION, 0);
    // Through the summoned portal: nothing.
    let from = h.bots[a].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &at(80.0));
    h.run(10);
    assert!(
        !calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
        "a summoned portal is walked through"
    );
    // Just short of touching (1.45 m out), then touching (1.40 m).
    h.bots[a].send(true, queue::WEENIE, &at(71.45));
    h.run(10);
    assert!(
        !calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
        "not yet touching"
    );
    h.bots[a].send(true, queue::WEENIE, &at(71.40));
    h.run(10);
    assert_eq!(
        calls(&h, a, from),
        [
            SHOW_TEXT,
            trip::ANIMATION_FROZEN,
            MOVEMENT_FROZEN,
            trip::ENTER_PORTAL_SPACE
        ],
        "into portal space"
    );
    h.run(40);
    assert!(calls(&h, a, from).ends_with(&[
        trip::EXIT_PORTAL_SPACE,
        trip::ANIMATION_FROZEN,
        trip::USE_DONE
    ]));
    assert_eq!(
        h.server.players.values().next().expect("in").position.cell,
        0x7000_0001,
        "taken"
    );
    // Put back in touch with it: still touching since it fired, so nothing; away and back
    // again, it takes the player again.
    let world = *h.server.players.keys().next().expect("in");
    let place = |h: &mut Harness, x: f32| {
        let clock = h.clock();
        if let Some(p) = h.server.players.get_mut(&world) {
            p.position.cell = cell;
            p.position.origin = [x, 240.0, -44.6];
        }
        yalain_world::places::walked(&mut h.server, &mut h.net, world, clock);
    };
    let from = h.bots[a].inbox.len();
    place(&mut h, 71.0);
    h.run(5);
    assert!(
        !calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
        "not again while touching"
    );
    place(&mut h, 75.0);
    place(&mut h, 71.0);
    h.run(5);
    assert!(
        calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
        "again after leaving"
    );
}

/// Using a door played as a scene: in one tick the teleporting text, the animation and movement
/// frozen and the door's scene begun (whole, playable), with no flash and no glows; a second and
/// a half later the scene ended, the animation freed, the use answered and the player placed at
/// the destination with a newer teleport stamp. The client drawing the new place frees it.
/// Behaviour: server.places.a-door-plays-its-scene
#[test]
fn a_door_plays_its_scene_in_place_of_portal_space_and_takes_the_player_through() {
    const DOOR: u64 = 0x4002_9900_0000_0058;
    const SCENE: u32 = 0x4000_005A;
    let mut h = Harness::new();
    let to = Place {
        cell: 0xFEFB_0100,
        origin: [0.0, 4.2, 100.16],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let door = object(DOOR, 0x4700_1178, [61.0, 240.2, -44.6], Some(to));
    h.server
        .story
        .doors
        .insert((door.cell, door.template), SCENE);
    h.server.story.scenes.insert(
        SCENE,
        yalain_content::story::Scene {
            id: SCENE,
            act: 0x4000_000C,
            number: 28,
            name: (0xD2F7_ECB9, 0x2500_18A7),
            hidden: false,
            playable: true,
            keyframes: vec![yalain_content::story::Keyframe {
                seconds: 12.0,
                picture: 0x4100_4BD7,
                subtitle: Some((0xD2F7_ECB9, 0x2500_18A7)),
                tracks: 0x11,
                ..yalain_content::story::Keyframe::default()
            }],
        },
    );
    h.server.content = ContentPack {
        objects: vec![door],
        ..ContentPack::default()
    };
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let fa = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(DOOR, ida)]),
    );
    h.run(10);
    assert_eq!(
        calls(&h, a, fa),
        [
            SHOW_TEXT,
            trip::ANIMATION_FROZEN,
            MOVEMENT_FROZEN,
            trip::ENTER_PORTAL_SCENE
        ],
        "into the door's scene"
    );
    let scene_sent = h.world_messages(a, fa).iter().any(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == trip::ENTER_PORTAL_SCENE => c
            .args
            .as_ref()
            .and_then(|x| yalain_world::calls::values(&x.0).ok())
            .is_some_and(|v| matches!(v.first(), Some(yalain_world::calls::Value::Graph(_)))),
        _ => false,
    });
    assert!(scene_sent, "the scene whole, first");
    assert!(
        !h.world_messages(a, fa)
            .iter()
            .any(|m| matches!(m, ServerMessage::DoFx(_))),
        "no flash, no glows"
    );
    h.run(40);
    assert!(calls(&h, a, fa).ends_with(&[
        trip::EXIT_PORTAL_SCENE,
        trip::ANIMATION_FROZEN,
        trip::USE_DONE
    ]));
    assert_eq!(teleported_to(&h, a, fa, ida), Some((0xFEFB_0100, 1)));
    let fb = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &auberean_protocol::ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    assert!(calls(&h, a, fb).contains(&MOVEMENT_FROZEN), "freed");
}

/// A use's answers since `from`: (status, control flags, distance).
fn use_answers(h: &Harness, b: usize, from: usize) -> Vec<(u32, u32, f32)> {
    use yalain_world::calls::{self, Node, Value};
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == trip::USE_DONE => {
                let v = c.args.as_ref().and_then(|a| calls::values(&a.0).ok())?;
                let Some(Value::Graph(g)) = v.first() else {
                    return None;
                };
                g.iter().find_map(|(_, n)| match n {
                    Node::Script { slots, .. } => {
                        Some((slots[11].1, slots[17].1, f32::from_bits(slots[8].1)))
                    }
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

/// A shown quest of the test world, for a portal to need or give.
fn portal_quest(id: u32) -> yalain_content::quests::QuestDef {
    yalain_content::quests::QuestDef {
        id,
        name: (0xA860_7CEC, 0x2500_17EE),
        description: (0x6C45_9364, 0x2500_17EE),
        icon: 0,
        play_fx: false,
        journals: Vec::new(),
        challenge: -999,
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
        last_waits: false,
        hero_only: false,
        pay: yalain_content::quests::CompletionPay::default(),
        required: None,
    }
}

/// A portal its quest keeps the player out of is refused at once wherever the player stands, near
/// or 19 m off, with no walk and no portal space: lacking the quest (10019), the answer's flags 820
/// and the distance to the portal; with the quest under way it takes the player.
/// Behaviour: server.places.portal-kept-by-a-quest
#[test]
fn a_portal_its_quest_keeps_the_player_out_of_is_refused_at_once() {
    use yalain_content::{Prop, Props};
    use yalain_world::quests::status;
    const STOCKPILE: u32 = 0x4700_12DD;
    const FAR: u64 = 0x4002_9900_0000_0058;
    const ENTER: u32 = 0x8000_0102;
    let mut h = Harness::new();
    let mut props = Props::default();
    props
        .set("Usage_RequiredQuest", Prop::Word(ENTER))
        .set("Usage_RequiredQuestStatus", Prop::Word(status::UNDERWAY));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STOCKPILE, 0x1A00, props));
    h.server.quests.quests = vec![portal_quest(ENTER)];
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = ContentPack {
        objects: vec![
            object(PORTAL, STOCKPILE, [61.0, 240.2, -44.6], Some(to)),
            object(FAR, STOCKPILE, [61.0, 259.2, -44.6], Some(to)),
        ],
        ..ContentPack::default()
    };
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    for (portal, near) in [(PORTAL, true), (FAR, false)] {
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::USE, &[use_of(portal, me)]),
        );
        h.run(10);
        let answers = use_answers(&h, a, from);
        assert_eq!(answers.len(), 1, "answered once");
        let (st, flags, distance) = answers[0];
        assert_eq!((st, flags), (10_019, 820), "lacking the quest");
        assert!(
            if near {
                distance < 3.0
            } else {
                distance > 15.0
            },
            "the distance to the portal: {distance}"
        );
        assert!(
            !calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
            "no trip"
        );
        assert!(
            !h.world_messages(a, from)
                .iter()
                .any(|m| matches!(m, ServerMessage::MoveTo(_))),
            "no walk"
        );
    }
    for p in h.server.players.values_mut() {
        p.play.quests = vec![yalain_world::state::QuestState {
            quest: ENTER,
            status: status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        }];
    }
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, me)]),
    );
    h.run(50);
    assert!(calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE));
    assert_eq!(teleported_to(&h, a, from, me), Some((0x7000_0001, 1)));
}

/// A portal's own effects are put on the player as it comes out of portal space, before the use
/// is answered: a portal that gives a quest tells it given between the exit and the answer.
/// Behaviour: server.places.portal-effects-on-arrival
#[test]
fn a_portals_effects_are_put_on_as_the_player_comes_out() {
    use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
    const CHAOS_PORTAL: u32 = 0x4700_131E;
    const GIVES: u32 = 0x6F00_3E61;
    const QUEST: u32 = 0x4000_02A5;
    const UPDATE_QUEST: u32 = 0x8213_0024;
    let mut h = Harness::new();
    h.server.catalog.insert(yalain_testkit::test_template(
        CHAOS_PORTAL,
        0x1A00,
        yalain_content::Props::default(),
    ));
    h.server.item_uses.insert(
        CHAOS_PORTAL,
        vec![yalain_world::item_use::UsageEffect {
            state: GIVES,
            spellcraft: 1.0,
        }],
    );
    h.server.effect_table.effects.insert(
        GIVES,
        EffectDef {
            state: GIVES,
            kind: EffectKind::Quest {
                quest: QUEST,
                update: 0x1000_0000,
            },
            duration: Scale::default(),
            priority: Scale::default(),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            enum_value: 0,
            eq_class: 0,
            name: None,
        },
    );
    h.server.quests.quests = vec![portal_quest(QUEST)];
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = ContentPack {
        objects: vec![object(PORTAL, CHAOS_PORTAL, [61.0, 240.2, -44.6], Some(to))],
        ..ContentPack::default()
    };
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, me)]),
    );
    h.run(10);
    assert!(
        !calls(&h, a, from).contains(&UPDATE_QUEST),
        "nothing given on the way in"
    );
    h.run(40);
    assert!(calls(&h, a, from).ends_with(&[
        trip::EXIT_PORTAL_SPACE,
        trip::ANIMATION_FROZEN,
        UPDATE_QUEST,
        trip::USE_DONE
    ]));
    let p = h.server.players.values().next().expect("in the world");
    assert!(
        p.play
            .quests
            .iter()
            .any(|q| q.quest == QUEST && q.status == yalain_world::quests::status::UNDERWAY),
        "the quest given"
    );
}

/// A placed portal for heroes alone refuses anyone else, at a use and a walk-in, 10049 with flags
/// 820 and no trip; a hero it takes.
/// Behaviour: server.places.portal-hero-only
#[test]
fn a_placed_portal_for_heroes_alone_refuses_anyone_else_at_a_use_and_a_walk_in() {
    use auberean_primitives::Vec3;
    use yalain_testkit::play::report;
    use yalain_world::world::IDLE_MOTION;
    const GATEWAY: u32 = 0x4700_115B;
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let mut gated = object(PORTAL, GATEWAY, [61.0, 259.2, -44.6], Some(to));
    gated.facts.hero = true;
    gated.description.package = 0x1A01;
    h.server.content = ContentPack {
        objects: vec![gated],
        ..ContentPack::default()
    };
    h.server.walk_in_packages.insert(0x1A01);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[a].inbox.len();
    let asks = server_call(function::USE, &[use_of(PORTAL, me)]);
    h.bots[a].send(true, queue::WEENIE, &asks);
    h.run(10);
    let answers = use_answers(&h, a, from);
    assert_eq!(answers.len(), 1, "answered once");
    assert_eq!((answers[0].0, answers[0].1), (10_049, 820));
    assert!(!calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE));
    let cell = h.server.players.values().next().expect("in").position.cell;
    let at = |x: f32, y: f32| report(cell, Vec3::new(x, y, -44.6), 0, IDLE_MOTION, 0);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &at(61.0, 258.2));
    h.run(10);
    h.bots[a].send(true, queue::WEENIE, &at(61.0, 258.21));
    h.run(10);
    let answers = use_answers(&h, a, from);
    assert_eq!(answers.len(), 1, "a walk-in is answered once");
    assert_eq!((answers[0].0, answers[0].1), (10_049, 820));
    assert!(!calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE));
    h.run(100);
    for p in h.server.players.values_mut() {
        yalain_world::focus::set_hero_kept(&mut p.qualities, true);
    }
    let from = h.bots[a].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &asks);
    h.run(40);
    assert!(
        calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
        "a hero is taken"
    );
}

/// A placed portal of one kingdom refuses a player of no kingdom or another, 10011 with flags 820
/// and no trip; one of that kingdom it takes.
/// Behaviour: server.places.portal-kingdom
#[test]
fn a_placed_portal_of_a_kingdom_refuses_anyone_not_of_it() {
    use auberean_primitives::Vec3;
    use yalain_testkit::play::report;
    use yalain_world::kingdoms::{kingdom, set_kingdom};
    use yalain_world::world::IDLE_MOTION;
    const GATEWAY: u32 = 0x4700_115B;
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let mut gated = object(PORTAL, GATEWAY, [61.0, 259.2, -44.6], Some(to));
    gated.facts.kingdom = Some(kingdom::ORDER);
    h.server.content = ContentPack {
        objects: vec![gated],
        ..ContentPack::default()
    };
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let asks = server_call(function::USE, &[use_of(PORTAL, me)]);
    for (who, k) in [("no kingdom", None), ("another", Some(kingdom::SHADOW))] {
        if let Some(k) = k {
            for p in h.server.players.values_mut() {
                set_kingdom(&mut p.qualities, k);
            }
        }
        let from = h.bots[a].inbox.len();
        h.bots[a].send(true, queue::WEENIE, &asks);
        h.run(10);
        let answers = use_answers(&h, a, from);
        assert_eq!(answers.len(), 1, "{who}: answered once");
        assert_eq!((answers[0].0, answers[0].1), (10_011, 820), "{who}");
        assert!(
            !calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
            "{who}: no trip"
        );
    }
    // One that leads nowhere keeps the same player out the same way.
    let mut nowhere = object(PORTAL, GATEWAY, [61.0, 259.2, -44.6], None);
    nowhere.facts.kingdom = Some(kingdom::ORDER);
    let saved = std::mem::replace(&mut h.server.content.objects, vec![nowhere]);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &asks);
    h.run(10);
    let answers = use_answers(&h, a, from);
    assert_eq!(answers.len(), 1, "leading nowhere: answered once");
    assert_eq!(
        (answers[0].0, answers[0].1),
        (10_011, 820),
        "leading nowhere"
    );
    h.server.content.objects = saved;
    for p in h.server.players.values_mut() {
        set_kingdom(&mut p.qualities, kingdom::ORDER);
    }
    let cell = h.server.players.values().next().expect("in").position.cell;
    let at = report(cell, Vec3::new(61.0, 258.2, -44.6), 0, IDLE_MOTION, 0);
    h.bots[a].send(true, queue::WEENIE, &at);
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &asks);
    h.run(40);
    assert!(
        calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
        "one of that kingdom is taken"
    );
}

/// A gate on a quest had in any way lets through a player with it failed, completed or under way;
/// one who never had it hears 10019, one who cancelled it 10020.
/// Behaviour: server.places.portal-quest-had
#[test]
fn a_gate_on_a_quest_had_lets_failed_completed_and_under_way_through() {
    use yalain_world::item_use::quest_refusal;
    use yalain_world::quests::status;
    const QUEST: u32 = 0x4000_00C5;
    let had = |s: u32| {
        vec![yalain_world::state::QuestState {
            quest: QUEST,
            status: s,
            phase: 0,
            started: 0,
            updated: 0,
        }]
    };
    let at = |held: &[yalain_world::state::QuestState]| {
        quest_refusal(held, QUEST, yalain_content::QuestGate::HAD, 0.0, 0)
    };
    assert_eq!(at(&[]), Some(10_019));
    assert_eq!(at(&had(status::UNDERWAY)), None);
    assert_eq!(at(&had(status::FAILED)), None);
    assert_eq!(at(&had(status::COMPLETED)), None);
    assert_eq!(at(&had(status::CANCELLED)), Some(10_020));
}

/// A placed portal's least level refuses a player below it, as a quest does: a use at once with no
/// walk and no trip, a walk-in, each 10009 with flags 820; at that level it takes the player.
/// Behaviour: server.places.portal-least-level
#[test]
fn a_placed_portal_refuses_a_player_below_its_least_level_at_a_use_and_a_walk_in() {
    use auberean_primitives::Vec3;
    use yalain_testkit::play::report;
    use yalain_world::world::IDLE_MOTION;
    const GATEWAY: u32 = 0x4700_115B;
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let mut gated = object(PORTAL, GATEWAY, [61.0, 259.2, -44.6], Some(to));
    gated.facts.min_level = Some(6);
    gated.description.package = 0x1A01;
    h.server.content = ContentPack {
        objects: vec![gated],
        ..ContentPack::default()
    };
    h.server.walk_in_packages.insert(0x1A01);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 302, 5);
    }
    let refused = |h: &Harness, from: usize, what: &str| {
        let answers = use_answers(h, a, from);
        assert_eq!(answers.len(), 1, "{what}: answered once");
        assert_eq!((answers[0].0, answers[0].1), (10_009, 820), "{what}");
        assert!(
            !calls(h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
            "{what}: no trip"
        );
    };
    let from = h.bots[a].inbox.len();
    let asks = server_call(function::USE, &[use_of(PORTAL, me)]);
    h.bots[a].send(true, queue::WEENIE, &asks);
    h.run(10);
    refused(&h, from, "a use");
    let cell = h.server.players.values().next().expect("in").position.cell;
    let at = |x: f32, y: f32| report(cell, Vec3::new(x, y, -44.6), 0, IDLE_MOTION, 0);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &at(61.0, 258.2));
    h.run(10);
    h.bots[a].send(true, queue::WEENIE, &at(61.0, 258.21));
    h.run(10);
    refused(&h, from, "a walk-in");
    for p in h.server.players.values_mut() {
        yalain_world::quals::set_int(&mut p.qualities, 302, 6);
    }
    let from = h.bots[a].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &asks);
    h.run(60);
    assert!(
        calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
        "at its level, the trip"
    );
}

/// A generic portal its placed instance keeps closed by a quest (its template needs none) is
/// refused as a portal its template keeps closed is: a use from 19 m at once, with no walk and no
/// trip, and a walk-in, each 10043 for a hidden quest with flags 820 and the distance to the
/// portal, even where the portal leads nowhere known; its twin of the same template with no gate
/// takes the player, and once the quest is done the gated one does too.
/// Behaviour: server.places.portal-instance-kept-by-a-quest
#[test]
fn a_portal_its_placed_instance_keeps_closed_by_a_quest_is_refused_at_a_use_and_a_walk_in() {
    use auberean_primitives::Vec3;
    use yalain_content::QuestGate;
    use yalain_testkit::play::report;
    use yalain_world::quests::status;
    use yalain_world::world::IDLE_MOTION;
    const VAULT: u32 = 0x4700_1154;
    const BOSS: u32 = 0x4000_004C;
    const TWIN: u64 = 0x4002_9900_0000_0058;
    const NOWHERE: u64 = 0x4002_9900_0000_0059;
    let mut h = Harness::new();
    h.server.quests.quests = vec![yalain_content::quests::QuestDef {
        hidden: true,
        ..portal_quest(BOSS)
    }];
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let gate = Some(QuestGate {
        quest: BOSS,
        status: status::COMPLETED,
    });
    let mut gated = object(PORTAL, VAULT, [61.0, 259.2, -44.6], Some(to));
    gated.gate = gate;
    gated.description.package = 0x1A01;
    let twin = object(TWIN, VAULT, [61.0, 240.2, -44.6], Some(to));
    let mut nowhere = object(NOWHERE, VAULT, [90.0, 240.0, -44.6], None);
    nowhere.gate = gate;
    nowhere.description.package = 0x1A01;
    h.server.content = ContentPack {
        objects: vec![gated, twin, nowhere],
        ..ContentPack::default()
    };
    h.server.walk_in_packages.insert(0x1A01);
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let refused = |h: &Harness, from: usize, far: bool, what: &str| {
        let answers = use_answers(h, a, from);
        assert_eq!(answers.len(), 1, "{what}: answered once");
        let (st, flags, distance) = answers[0];
        assert_eq!(
            (st, flags),
            (10_043, 820),
            "{what}: kept out by a hidden quest"
        );
        assert!(
            if far { distance > 15.0 } else { distance < 3.0 },
            "{what}: the distance to the portal: {distance}"
        );
        assert!(
            !calls(h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
            "{what}: no trip"
        );
        assert!(
            !h.world_messages(a, from)
                .iter()
                .any(|m| matches!(m, ServerMessage::MoveTo(_))),
            "{what}: no walk"
        );
    };
    for (portal, what) in [
        (PORTAL, "the gated portal"),
        (NOWHERE, "the gated portal to nowhere known"),
    ] {
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::USE, &[use_of(portal, me)]),
        );
        h.run(10);
        refused(&h, from, true, what);
    }
    // Walked into: refused alike, once while touching.
    let cell = h.server.players.values().next().expect("in").position.cell;
    let at = |x: f32, y: f32| report(cell, Vec3::new(x, y, -44.6), 0, IDLE_MOTION, 0);
    for (x, y, what) in [
        (61.0, 258.2, "walked into"),
        (90.0, 241.0, "walked into, to nowhere"),
    ] {
        let from = h.bots[a].inbox.len();
        h.bots[a].send(true, queue::WEENIE, &at(x, y));
        h.run(10);
        h.bots[a].send(true, queue::WEENIE, &at(x, y + 0.01));
        h.run(10);
        refused(&h, from, false, what);
    }
    // The same template with no gate takes the player.
    for p in h.server.players.values_mut() {
        p.position.origin = [61.0, 238.0, -44.6];
    }
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(TWIN, me)]),
    );
    h.run(50);
    assert!(
        calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
        "the twin takes the player"
    );
    // With the boss's quest done, the gated one takes the player too.
    for p in h.server.players.values_mut() {
        p.position.cell = cell;
        p.position.origin = [61.0, 256.0, -44.6];
        p.play.quests = vec![yalain_world::state::QuestState {
            quest: BOSS,
            status: status::COMPLETED,
            phase: 0,
            started: 0,
            updated: 0,
        }];
    }
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, me)]),
    );
    h.run(50);
    assert!(
        calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
        "open once done"
    );
}

/// A placed instance's gate asks its quest where the row puts it, as a template's does: under way
/// at a phase (a later phase refused 10020), under way at any phase, or completed; a shown quest
/// never had is refused 10019.
/// Behaviour: server.places.portal-instance-gate-statuses
#[test]
fn a_placed_instances_gate_asks_its_quest_at_a_phase_under_way_or_completed() {
    use yalain_content::QuestGate;
    use yalain_world::quests::status;
    const BASEMENT: u32 = 0x8000_038D;
    let mut h = Harness::new();
    h.server.quests.quests = vec![portal_quest(BASEMENT)];
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    let mut portal = object(PORTAL, 0x4700_1154, [61.0, 240.2, -44.6], Some(to));
    h.server.content = ContentPack {
        objects: vec![portal.clone()],
        ..ContentPack::default()
    };
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h.server.players.keys().next().expect("in");
    // (the gate's status, the quest as the player holds it, the answer: 0 a trip).
    let cases = [
        (status::UNDERWAY | 2, None, 10_019),
        (status::UNDERWAY | 2, Some((status::UNDERWAY, 3)), 10_020),
        (status::UNDERWAY | 2, Some((status::COMPLETED, 0)), 10_020),
        (status::UNDERWAY | 2, Some((status::UNDERWAY, 2)), 0),
        (status::UNDERWAY, Some((status::UNDERWAY, 4)), 0),
        (status::UNDERWAY, Some((status::FAILED, 0)), 10_020),
        (status::COMPLETED, Some((status::UNDERWAY, 1)), 10_020),
        (status::COMPLETED, Some((status::COMPLETED, 0)), 0),
    ];
    for (needed, held, answer) in cases {
        portal.gate = Some(QuestGate {
            quest: BASEMENT,
            status: needed,
        });
        h.server.content.objects[0] = portal.clone();
        if let Some(p) = h.server.players.get_mut(&world) {
            p.play.quests = held
                .map(|(st, phase)| yalain_world::state::QuestState {
                    quest: BASEMENT,
                    status: st,
                    phase,
                    started: 0,
                    updated: 0,
                })
                .into_iter()
                .collect();
            p.position.cell = 0x0299_0109;
            p.position.origin = [61.0, 238.0, -44.6];
        }
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::USE, &[use_of(PORTAL, me)]),
        );
        h.run(50);
        let what = format!("needs {needed:08X}, held {held:?}");
        if answer == 0 {
            assert!(
                calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
                "{what}: taken"
            );
        } else {
            let answers = use_answers(&h, a, from);
            assert_eq!(answers.first().map(|x| x.0), Some(answer), "{what}");
            assert!(
                !calls(&h, a, from).contains(&trip::ENTER_PORTAL_SPACE),
                "{what}: kept"
            );
        }
    }
}

/// A placed object of the script library's person kind with nothing to say (no line, no quest, no
/// store) is answered as a person is: near, at once, status 0, flags 956, and nothing said; far
/// off, the player is walked to within 5 m first. An object of another kind is still not answered.
/// Behaviour: server.places.person-kind-answers
#[test]
fn a_person_kind_object_with_nothing_to_say_answers_as_a_person() {
    use yalain_world::places::USE_FLAGS_PERSON;
    const BOARD: u32 = 0x4700_33ED;
    const NEAR: u64 = 0x4002_9900_0000_0060;
    const FAR: u64 = 0x4002_9900_0000_0061;
    const OTHER: u64 = 0x4002_9900_0000_0062;
    let mut h = Harness::new();
    let mut near = object(NEAR, BOARD, [61.0, 240.2, -44.6], None);
    near.description.package = 0x13A3;
    let mut far = object(FAR, BOARD, [61.0, 259.0, -44.6], None);
    far.description.package = 0x13A3;
    let other = object(OTHER, 0x4700_7777, [62.0, 240.2, -44.6], None);
    h.server.content = ContentPack {
        objects: vec![near, far, other],
        ..ContentPack::default()
    };
    h.server.person_packages.insert(0x13A3);
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let use_it = |h: &mut Harness, id: u64| {
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::USE, &[use_of(id, ida)]),
        );
        h.run(10);
        from
    };
    let from = use_it(&mut h, NEAR);
    let answers = use_answers(&h, a, from);
    assert_eq!(answers.len(), 1);
    assert_eq!((answers[0].0, answers[0].1), (0, USE_FLAGS_PERSON));
    assert!(!calls(&h, a, from).contains(&SHOW_TEXT), "nothing said");
    let from = use_it(&mut h, OTHER);
    assert!(
        use_answers(&h, a, from).is_empty(),
        "another kind: no answer"
    );
    let from = use_it(&mut h, FAR);
    assert!(use_answers(&h, a, from).is_empty(), "not yet: walked first");
    let walk = h.world_messages(a, from).iter().find_map(|m| match m {
        ServerMessage::MoveTo(o) if o.sender.id == ida => Some(o.action.params.desired_distance),
        _ => None,
    });
    assert_eq!(walk, Some(5.0));
}

/// A faction shrine: the player is walked to within 3 m of it, the walk stopped, the shrine and the
/// player play their behaviors, and "Your Kingdom Rating has been recorded."; the use is never
/// answered.
/// Behaviour: server.places.faction-shrine
#[test]
fn a_faction_shrine_records_the_rating_and_never_answers() {
    use yalain_world::calls::{self, Node};
    use yalain_world::usable::shrines::{RECORDED, SHRINE_BEHAVIOR, USER_BEHAVIOR};
    const SHRINE: u64 = 0x4002_9900_0000_0063;
    let mut h = Harness::new();
    let mut shrine = object(SHRINE, 0x4700_100D, [61.0, 240.2, -44.6], None);
    shrine.description.package = 0x13B0;
    h.server.content = ContentPack {
        objects: vec![shrine],
        ..ContentPack::default()
    };
    h.server.shrine_packages.insert(0x13B0);
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(SHRINE, ida)]),
    );
    h.run(10);
    let msgs = h.world_messages(a, from);
    let walks: Vec<f32> = msgs
        .iter()
        .filter_map(|m| match m {
            ServerMessage::MoveTo(o) if o.sender.id == ida => {
                Some(o.action.params.desired_distance)
            }
            _ => None,
        })
        .collect();
    assert_eq!(walks.first(), Some(&3.0), "walked to within 3 m: {walks:?}");
    let played = |who: u64, b: u32| {
        msgs.iter().any(|m| {
            matches!(m, ServerMessage::PlayBehavior(e) if e.sender.id == who && e.action.behavior == b)
        })
    };
    assert!(played(SHRINE, SHRINE_BEHAVIOR) && played(ida, USER_BEHAVIOR));
    let told = msgs.iter().any(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => c
            .args
            .as_ref()
            .and_then(|x| calls::values(&x.0).ok())
            .is_some_and(|v| {
                v.iter().any(|x| {
                    matches!(x.root(), Some(Node::TextInfo(t)) if (t.string_id, t.table) == RECORDED)
                })
            }),
        _ => false,
    });
    assert!(told, "the rating recorded");
    h.run(40);
    assert!(use_answers(&h, a, from).is_empty(), "never answered");
}

/// A faction shrine used on a mount: the rating recorded, then "Shrine use canceled."; on foot, the
/// rating alone.
/// Behaviour: server.places.faction-shrine-mounted
#[test]
fn a_faction_shrine_used_on_a_mount_is_canceled_after_the_rating() {
    use yalain_world::calls::{self, Node};
    use yalain_world::usable::shrines::{CANCELED, RECORDED};
    const SHRINE: u64 = 0x4002_9900_0000_0063;
    let mut h = Harness::new();
    let mut shrine = object(SHRINE, 0x4700_100D, [61.0, 240.2, -44.6], None);
    shrine.description.package = 0x13B0;
    h.server.content = ContentPack {
        objects: vec![shrine],
        ..ContentPack::default()
    };
    h.server.shrine_packages.insert(0x13B0);
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let shown = |h: &mut Harness| {
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::USE, &[use_of(SHRINE, ida)]),
        );
        h.run(10);
        h.world_messages(a, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::PlayerCall(c) if c.function.0 == SHOW_TEXT => c
                    .args
                    .as_ref()
                    .and_then(|x| calls::values(&x.0).ok())
                    .and_then(|v| {
                        v.iter().find_map(|x| match x.root() {
                            Some(Node::TextInfo(t)) => Some((t.string_id, t.table)),
                            _ => None,
                        })
                    }),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(shown(&mut h), vec![RECORDED], "on foot");
    for p in h.server.players.values_mut() {
        p.riding = Some(yalain_world::mounts::Riding {
            eid: 1,
            state: 0,
            look: auberean_protocol::types::VisualDesc::default(),
            speed: 1.0,
            checking: false,
        });
    }
    assert_eq!(shown(&mut h), vec![RECORDED, CANCELED], "mounted");
}

/// A player's report of its place carrying the moved-by-hand flag (or not) and a walk's stamp.
fn hand_report(
    at: yalain_store::StoredPosition,
    dx: f32,
    by_hand: bool,
    move_to_stamp: u16,
) -> auberean_protocol::ClientMessage {
    auberean_protocol::ClientMessage::Position(auberean_protocol::physics::ClientPosition {
        time: 0.0,
        cell: at.cell,
        origin: auberean_primitives::Vec3::new(at.origin[0] + dx, at.origin[1], at.origin[2]),
        heading_motion: yalain_world::world::IDLE_MOTION,
        flags: if by_hand {
            yalain_world::places::MOVED_BY_HAND
        } else {
            0
        },
        instance_stamp: 1,
        teleport_stamp: 0,
        force_position_stamp: 0,
        move_to_stamp,
        jump_velocity: auberean_primitives::Vec3::ZERO,
    })
}

/// A player walked toward a portal that moves by its own hand once the walk is taken up (its
/// report carries the walk's stamp) has interrupted it: the walk is stopped and the use refused
/// (54, flags 820) at the distance it now stands, and it is free to act again. A report under the
/// walk, or one by hand from before the walk was taken up, changes nothing.
/// Behaviour: server.places.walk-interrupted
#[test]
fn a_walk_to_a_portal_the_player_moves_off_by_hand_is_refused_as_interrupted() {
    use yalain_world::places::{MOVE_INTERRUPTED, USE_FLAGS_BOUND};
    let mut h = Harness::new();
    let to = Place {
        cell: 0x7000_0001,
        origin: [10.0, 10.0, 50.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    h.server.content = ContentPack {
        objects: vec![object(PORTAL, 0x4700_1154, [61.0, 259.0, -44.6], Some(to))],
        ..ContentPack::default()
    };
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == ida)
        .expect("in the world")
        .0;
    let at = h.server.players[&world].position;
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(PORTAL, ida)]),
    );
    h.run(10);
    let walk = h
        .world_messages(a, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::MoveTo(o) if o.sender.id == ida && o.action.params.flags == 0x63 => {
                Some(o.action.stamp)
            }
            _ => None,
        })
        .expect("walked toward the portal");
    // Under the walk, then by hand from before the walk was taken up: still walking.
    h.bots[a].send(true, queue::WEENIE, &hand_report(at, 0.0, false, walk));
    h.run(4);
    h.bots[a].send(
        true,
        queue::WEENIE,
        &hand_report(at, 0.0, true, walk.wrapping_sub(1)),
    );
    h.run(4);
    assert!(use_answers(&h, a, from).is_empty(), "still walking");
    let before = h.bots[a].inbox.len();
    h.bots[a].send(true, queue::WEENIE, &hand_report(at, -10.0, true, walk));
    h.run(4);
    let answers = use_answers(&h, a, before);
    assert_eq!(answers.len(), 1, "refused once");
    assert_eq!(
        (answers[0].0, answers[0].1),
        (MOVE_INTERRUPTED, USE_FLAGS_BOUND)
    );
    assert!(answers[0].2 > 20.0, "at the distance it now stands");
    let stops: Vec<u32> = h
        .world_messages(a, before)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::MoveTo(o) if o.sender.id == ida => Some(o.action.params.flags),
            _ => None,
        })
        .collect();
    assert_eq!(stops, [0x40], "the walk stopped");
    assert!(
        !yalain_world::places::actions_locked(&h.server, world),
        "free to act again"
    );
    assert_eq!(teleported_to(&h, a, from, ida), None, "no trip");
}

/// A placed object for monarchs alone is refused at once wherever the player stands, with no walk,
/// to a player in no allegiance and to a vassal: "Only monarchs can use that item." (10038), flags
/// 828, the distance from where the player asked. A monarch is not refused.
/// Behaviour: server.places.monarch-only
#[test]
fn an_object_for_monarchs_alone_is_refused_at_once_to_anyone_else() {
    use yalain_world::allegiance::Node;
    use yalain_world::item_use::refusal::MONARCH_ONLY;
    const SIGN: u64 = 0x4002_9900_0000_0064;
    const TEMPLATE: u32 = 0x4700_11C4;
    let mut h = Harness::new();
    let mut props = yalain_content::Props::default();
    props.set("Usage_MonarchOnly", yalain_content::Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(TEMPLATE, 0x13C1, props));
    h.server.content = ContentPack {
        objects: vec![object(SIGN, TEMPLATE, [61.0, 290.0, -44.6], None)],
        ..ContentPack::default()
    };
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let used = |h: &mut Harness| {
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::USE, &[use_of(SIGN, ida)]),
        );
        h.run(3);
        let walked = h
            .world_messages(a, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::MoveTo(o) if o.sender.id == ida));
        (use_answers(h, a, from), walked)
    };
    let (answers, walked) = used(&mut h);
    assert_eq!(answers.len(), 1, "in no allegiance: answered at once");
    assert_eq!((answers[0].0, answers[0].1), (MONARCH_ONLY, 828));
    assert_eq!(MONARCH_ONLY, 10_038);
    assert!(answers[0].2 > 40.0, "from where the player asked");
    assert!(!walked, "no walk");
    const PATRON: u64 = 0x2130_0000_0000_0FFF;
    let node = |patron: u64| Node {
        patron,
        ..Node::default()
    };
    h.server.allegiances.nodes.insert(PATRON, node(0));
    h.server.allegiances.nodes.insert(ida, node(PATRON));
    let (answers, _) = used(&mut h);
    assert_eq!(answers.len(), 1, "a vassal");
    assert_eq!(answers[0].0, MONARCH_ONLY);
    h.server.allegiances.nodes.remove(&PATRON);
    h.server.allegiances.nodes.insert(ida, node(0));
    let (answers, _) = used(&mut h);
    assert!(answers.is_empty(), "a monarch is not refused");
}

/// A placed object of a kind that cannot be used (a forge, a road sign) is refused at once wherever
/// the player stands, with no walk: "cannot use" (10001), flags 828, the distance from where the
/// player asked.
/// Behaviour: server.places.cannot-be-used
#[test]
fn a_forge_or_a_road_sign_is_refused_at_once_wherever_the_player_stands() {
    use yalain_world::places::{USE_FLAGS_CANNOT_BE_USED, USE_STATUS_IN_USE};
    const SIGN: u64 = 0x4002_9900_0000_0064;
    const OTHER: u64 = 0x4002_9900_0000_0065;
    let mut h = Harness::new();
    let mut sign = object(SIGN, 0x4700_1BE1, [61.0, 290.0, -44.6], None);
    sign.description.package = 0x13C0;
    let other = object(OTHER, 0x4700_7777, [62.0, 240.2, -44.6], None);
    h.server.content = ContentPack {
        objects: vec![sign, other],
        ..ContentPack::default()
    };
    h.server.cannot_use_packages.insert(0x13C0);
    let (a, ida) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(SIGN, ida)]),
    );
    h.run(3);
    let answers = use_answers(&h, a, from);
    assert_eq!(answers.len(), 1, "answered at once");
    assert_eq!((answers[0].0, answers[0].1), (USE_STATUS_IN_USE, 828));
    assert_eq!(USE_FLAGS_CANNOT_BE_USED, 828);
    assert!(answers[0].2 > 40.0, "from where the player asked");
    assert!(
        !h.world_messages(a, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::MoveTo(o) if o.sender.id == ida)),
        "no walk"
    );
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::USE, &[use_of(OTHER, ida)]),
    );
    h.run(3);
    assert!(
        use_answers(&h, a, from).is_empty(),
        "an object of another kind: not answered"
    );
}

/// Of what the world database places, creatures meet what stays where it is for every player: not
/// a creature, a person on a route, a door, or what a quest gates.
/// Behaviour: server.creatures.chase-around-walls
#[test]
fn creatures_meet_what_stays_where_it_stands_for_every_player() {
    let mut h = Harness::new();
    let plain = object(0x4002_9900_0000_0001, 0x4700_0001, [1.0, 2.0, 3.0], None);
    let mut scaled = object(0x4002_9900_0000_0002, 0x4700_0001, [1.0, 2.0, 3.0], None);
    scaled.description.scale = Some(2.5);
    let mut creature = object(0x4002_9900_0000_0003, 0x4700_0001, [1.0, 2.0, 3.0], None);
    creature.kind = Kind::Creature;
    let mut walker = object(0x4002_9900_0000_0004, 0x4700_0001, [1.0, 2.0, 3.0], None);
    walker.walks = Some(yalain_content::CampMoves::default());
    let mut gated = object(0x4002_9900_0000_0005, 0x4700_0001, [1.0, 2.0, 3.0], None);
    gated.gate = Some(yalain_content::QuestGate {
        quest: 0x4000_0001,
        status: 0,
    });
    let mut door = object(0x4002_9900_0000_0006, 0x4700_0001, [1.0, 2.0, 3.0], None);
    door.description.package = 0xD00;
    h.server
        .doors
        .packages
        .insert(0xD00, yalain_world::usable::doors::Kind::Plain);
    h.server.content.objects = vec![plain.clone(), scaled.clone(), creature, walker, gated, door];
    let rows = yalain_world::walls_standing(&h.server);
    let met: Vec<(u64, f32)> = rows.iter().map(|r| (r.id, r.scale)).collect();
    assert_eq!(
        met,
        [(plain.object.unwrap(), 1.0), (scaled.object.unwrap(), 2.5)]
    );
}

/// A placed person's name as literal text is what the client is told it is called, in place of its
/// template's, as a player's name is told.
/// Behaviour: none (a world's own name for a person)
#[test]
fn a_placed_things_name_text_is_told_as_its_name() {
    let mut o = object(0x4002_9900_0000_0001, 0x4700_0001, [1.0, 2.0, 3.0], None);
    o.description.name = Some((0x10, 0x25));
    o.description.name_text = Some("Old Tom".into());
    let made = yalain_world::player::content_object(1, &o);
    assert_eq!(
        made.weenie.name,
        auberean_protocol::text::StringInfo::literal("Old Tom")
    );
}
