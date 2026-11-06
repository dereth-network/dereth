//! Players seeing each other: made for each other as they come into view, their movements and
//! head turns relayed, forgotten as they leave; objects asked for again.

use auberean_primitives::Vec3;
use auberean_protocol::login::{CharacterRef, Empty};
use auberean_protocol::physics::{HeadDirection, Target};
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_content::{ContentObject, ContentPack, Description, Kind};
use yalain_testkit::invariants::VisualOrder;
use yalain_testkit::play::report;
use yalain_testkit::{Harness, TestGround};
use yalain_world::world::{motion, ARRIVAL_FX, IDLE_MOTION};

fn created(h: &Harness, b: usize, from: usize, id: u64) -> bool {
    h.world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::CreateObject(o) if o.id == id))
}

fn left(h: &Harness, b: usize, from: usize, id: u64) -> bool {
    h.world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::ObjectLeaves(e) if e.sender.id == id))
}

fn positions_of(h: &Harness, b: usize, from: usize, id: u64) -> usize {
    h.world_messages(b, from)
        .iter()
        .filter(|m| {
            matches!(m, ServerMessage::Position(p) | ServerMessage::PositionCell(p)
                if p.sender.id == id)
        })
        .count()
}

/// Two players entering the same place are made for each other: the one already there is made
/// for the newcomer as it enters, and the newcomer for it, named and placed as the world shows
/// them.
/// Behaviour: server.presence.players-made-for-each-other
#[test]
fn two_players_in_one_place_are_made_for_each_other() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let from_a = h.bots[a].inbox.len();
    let (b, idb) = h.player_in_world("two", "Babyfare");
    assert!(
        created(&h, b, 0, ida),
        "the newcomer is told of the one already there"
    );
    assert!(
        h.run_until(50, |h| created(h, a, from_a, idb)),
        "and the other way"
    );
    let o = h
        .world_messages(b, 0)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == ida => Some(o),
            _ => None,
        })
        .expect("made");
    assert_eq!(
        o.weenie.name.literal.map(|w| w.text()),
        Some("Elara [Mv]".to_string())
    );
    assert_eq!(o.physics.position.cell, 0x0299_0109);
}

/// A player's reported place reaches every client that knows it, as the player's position with
/// a newer position stamp each time, on that client's own stream for the player.
/// Behaviour: server.presence.positions-reach-observers
#[test]
fn a_players_reported_place_reaches_the_clients_that_know_it_with_newer_stamps() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = h.bots[b].inbox.len();
    for (i, x) in [60.0f32, 61.0, 62.5].into_iter().enumerate() {
        h.bots[a].send(
            true,
            queue::WEENIE,
            &report(
                0x0299_0109,
                Vec3::new(x, 240.0, -44.6),
                64,
                motion(127, 255, 127),
                0,
            ),
        );
        let seen = h.wait_for(b, from, |m| match m {
            ServerMessage::Position(p) if p.sender.id == ida && p.origin.x == x => Some(*p),
            _ => None,
        });
        let p = seen.unwrap_or_else(|| panic!("report {i} reaches the other client"));
        assert_eq!(
            p.heading_motion,
            64 << 24 | motion(127, 255, 127),
            "heading and motion as asked"
        );
    }
    let stamps: Vec<u16> = h
        .world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::Position(p) if p.sender.id == ida => Some(p.position_stamp),
            _ => None,
        })
        .collect();
    assert_eq!(stamps.len(), 3);
    assert!(
        stamps.windows(2).all(|w| w[1] > w[0]),
        "stamps rise: {stamps:?}"
    );
    let ephemeral = h.bots[b].inbox[from..]
        .iter()
        .filter(|r| matches!(r.message(), Some(ServerMessage::Position(_))))
        .all(|r| r.blob_id >> 56 == 0x83);
    assert!(ephemeral, "on an ephemeral stream");
}

/// A player's head turns and what it looks at reach the clients that know it.
/// Behaviour: server.presence.head-and-look-at-relayed
#[test]
fn head_turns_and_looking_at_reach_the_clients_that_know_the_player() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = h.bots[b].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &ClientMessage::TurnHead(HeadDirection { x: 0.25, y: -0.5 }),
    );
    let head = h.wait_for(b, from, |m| match m {
        ServerMessage::TurnHead(e) if e.sender.id == ida => Some(e.action),
        _ => None,
    });
    assert_eq!(head, Some(HeadDirection { x: 0.25, y: -0.5 }));
    h.bots[a].send(true, queue::WEENIE, &ClientMessage::LookAt(Target(idb)));
    let looked = h.wait_for(b, from, |m| match m {
        ServerMessage::LookAt(e) if e.sender.id == ida => Some(e.action.0),
        _ => None,
    });
    assert_eq!(looked, Some(idb));
}

/// The end of a player's arrival effect reaches a client already watching it, which then takes the
/// player's later visual messages at once.
/// Behaviour: server.presence.a-players-arrival-ends-for-all-who-see-it
#[test]
fn the_end_of_an_arrival_reaches_a_client_already_watching_and_nothing_after_it_waits() {
    let mut h = Harness::new();
    let (w, _) = h.player_in_world("two", "Babyfare");
    let (a, me) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[w].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(Empty),
    );
    let ended = h.wait_for(w, from, |m| match m {
        ServerMessage::StopFx(e) if e.sender.id == me => Some(e.action.0),
        _ => None,
    });
    assert_eq!(ended, Some(ARRIVAL_FX));
    h.bots[a].send(true, queue::WEENIE, &ClientMessage::LookAt(Target(me)));
    let looked = h.wait_for(w, from, |m| match m {
        ServerMessage::LookAt(e) if e.sender.id == me => Some(e.action.0),
        _ => None,
    });
    assert_eq!(looked, Some(me));
    let mut order = VisualOrder::new();
    for r in h.bots[w].inbox.iter().filter(|r| r.world) {
        order.see(r.blob_id, &r.bytes);
    }
    assert_eq!(order.held(), Vec::new(), "nothing waits for a stamp");
}

/// Taken as a client takes them, a visual message waits for the stamps before its own and for its
/// object; one at a stamp already passed is dropped; a visual stamp under a private message counts for
/// nothing, and is told as wrong.
/// Behaviour: server.presence.a-check-of-visual-order-waits-as-a-client-waits
#[test]
fn the_check_of_visual_order_waits_as_a_client_waits() {
    use auberean_protocol::physics::{ObjectEvent, Word};
    use auberean_protocol::types::ObjectStamp;
    use yalain_net::blob::{blob_id, BlobKind};
    use yalain_testkit::invariants::Held;
    const ID: u64 = 0x2130_0000_0000_0001;
    let stop = |other_stamp: u16| {
        ServerMessage::StopFx(ObjectEvent {
            sender: ObjectStamp {
                id: ID,
                instance_stamp: 1,
                other_stamp,
            },
            action: Word(7),
        })
        .encode()
    };
    let visual = blob_id(BlobKind::VISUAL, 1, 0, 0);
    let made = ServerMessage::CreateObject(auberean_protocol::physics::CreateObject {
        id: ID,
        physics: auberean_protocol::types::PhysicsDesc {
            instance_stamp: 1,
            visual_ordering_stamp: 5,
            ..Default::default()
        },
        ..Default::default()
    })
    .encode();
    let mut order = VisualOrder::new();
    order.see(visual, &stop(6));
    assert!(order.held().is_empty(), "waits for its object, not a stamp");
    order.see(blob_id(BlobKind::UNORDERED, 1, 0, 0), &made);
    assert!(order.held().is_empty(), "taken once its object is made");
    order.see(visual, &stop(8));
    let held = Held {
        object: ID,
        current: 6,
        stamp: 8,
        kind: "0x100a1".to_owned(),
    };
    assert_eq!(order.held(), [held]);
    order.see(visual, &stop(7));
    assert!(order.held().is_empty(), "the missing stamp frees it");
    order.see(visual, &stop(3));
    assert_eq!(order.dropped.get("0x100a1"), Some(&1), "passed: dropped");
    assert!(order.misrouted().is_empty());
    order.see(blob_id(BlobKind::PRIVATE, 1, 1, 0), &stop(9));
    assert_eq!(
        order.misrouted(),
        ["0x100a1"],
        "a visual stamp, sent private"
    );
}

/// A player that walks out of another's view is not taken away from that client (the client lets
/// it go itself); its movements stop reaching it, and it is made for it again when it comes back.
/// Behaviour: server.presence.out-of-view-let-go
#[test]
fn a_player_out_of_view_is_let_go_and_made_again_when_it_returns() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = h.bots[b].inbox.len();
    // Out of the hall to the outdoors far away (that move still reaches it), then about there.
    let mut moved_on = from;
    for x in [10.0f32, 12.0, 14.0] {
        h.bots[a].send(
            true,
            queue::WEENIE,
            &report(0x7000_0001, Vec3::new(x, 10.0, 50.0), 0, IDLE_MOTION, 0),
        );
        h.run(10);
        if x < 11.0 {
            moved_on = h.bots[b].inbox.len();
        }
    }
    assert_eq!(positions_of(&h, b, moved_on, ida), 0, "its movements stop");
    assert!(
        !left(&h, b, from, ida),
        "nothing is sent as it leaves the view"
    );
    assert!(
        !h.world_messages(b, from)
            .iter()
            .any(|m| matches!(m, ServerMessage::ForgetObject(_))),
        "nothing is sent as it leaves the view"
    );
    let from = h.bots[b].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &report(
            0x0299_0109,
            Vec3::new(59.0, 240.0, -44.6),
            0,
            IDLE_MOTION,
            0,
        ),
    );
    assert!(h.run_until(100, |h| created(h, b, from, ida)));
}

/// Reports that keep crossing the view's lines bring a player's view up to date no oftener than the
/// world's pace: the second, straight after the first, is seen once that pace has passed, not before.
/// Behaviour: server.presence.a-view-moves-at-the-worlds-pace
#[test]
fn reports_crossing_the_view_bring_it_up_to_date_at_the_worlds_pace() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(40);
    // Short steps, so that a report and all that follows it come within the pace.
    let reported = |h: &mut Harness, cell: u32| {
        for _ in 0..8 {
            h.step_by(0.002);
        }
        assert!(h
            .server
            .players
            .values()
            .any(|p| p.character == ida && p.position.cell == cell));
    };
    let start = h.now.0;
    h.bots[a].send(
        true,
        queue::WEENIE,
        &report(0x7000_0001, Vec3::new(10.0, 10.0, 50.0), 0, IDLE_MOTION, 0),
    );
    reported(&mut h, 0x7000_0001);
    let from = h.bots[b].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &report(
            0x0299_0109,
            Vec3::new(59.0, 240.0, -44.6),
            0,
            IDLE_MOTION,
            0,
        ),
    );
    reported(&mut h, 0x0299_0109);
    assert!(h.now.0 - start < yalain_world::presence::VIEW_EVERY);
    assert!(!created(&h, b, from, ida), "not yet");
    assert!(h.run_until(100, |h| created(h, b, from, ida)), "owed");
}

/// A player that leaves the world (at the end of its linger after a logoff) is seen to leave by
/// every client that held it, on its stream of the player's places, with the player's next
/// position stamp.
/// Behaviour: server.presence.leaving-seen
#[test]
fn a_player_leaving_the_world_is_seen_to_leave_by_the_others() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = h.bots[b].inbox.len();
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: ida }),
    );
    assert!(h.run_until(500, |h| left(h, b, from, ida)));
    let leave = h.bots[b].inbox[from..]
        .iter()
        .find(|r| matches!(r.message(), Some(ServerMessage::ObjectLeaves(_))))
        .expect("seen to leave");
    assert_eq!(leave.blob_id >> 56, 0x83, "ephemeral");
}

/// An object the client asks for again is made again, if it is in view.
/// Behaviour: server.presence.force-create-answered
#[test]
fn an_object_asked_for_again_is_made_again() {
    let mut h = Harness::new();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::ForceCreate(Target(ida)),
    );
    assert!(h.run_until(50, |h| created(h, b, from, ida)));
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &ClientMessage::ForceCreate(Target(idb)),
    );
    assert!(h.run_until(50, |h| created(h, a, from, idb)));
}

/// The hall's cells for the interior view: the entry cell sees the next one, which sees the entry
/// cell and a far one; the far one sees only the next one; a cell apart sees nothing.
const ENTRY: u32 = 0x0299_0109;
const NEXT: u32 = 0x0299_0100;
const FAR: u32 = 0x0299_0116;
const APART: u32 = 0x0299_014D;

fn hall_ground() -> TestGround {
    TestGround::seeing(&[
        (ENTRY, &[NEXT]),
        (NEXT, &[ENTRY, FAR]),
        (FAR, &[NEXT]),
        (APART, &[]),
    ])
}

fn scene_drawn(h: &mut Harness, b: usize) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(Empty),
    );
    h.run(5);
}

fn walk_to(h: &mut Harness, b: usize, cell: u32) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &report(cell, Vec3::new(20.0, 250.0, -44.6), 0, IDLE_MOTION, 0),
    );
    h.run(5);
}

fn static_object(id: u64, cell: u32) -> ContentObject {
    ContentObject {
        object: Some(id),
        template: 0x4700_1154,
        description: Description {
            name_text: None,
            package: 0xF0C,
            look: 0x1F00_031E,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0x30, 0, 0x220_0010, 0, 0x220_0010, 0],
            source: "authored for a test".into(),
        },
        cell,
        origin: [20.0, 250.0, -44.0],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: Kind::Static,
        label: "Portal".into(),
        placed: "authored for a test".into(),
        destination: None,
        facts: yalain_content::Facts::default(),
        walks: None,
        gate: None,
    }
}

/// Indoors, two players are made for each other only once each one's cell sees the other's
/// (its own cell, or a cell in its cell's visible list once its scene is drawn), never while
/// they stand in cells of the place that do not see each other; then both at once.
/// Behaviour: server.presence.interior-view-by-visible-cells
#[test]
fn players_in_one_interior_are_made_for_each_other_only_once_their_cells_see_each_other() {
    let mut h = Harness::new();
    h.server.ground = Some(Box::new(hall_ground()));
    let (a, ida) = h.player_in_world("one", "Elara");
    scene_drawn(&mut h, a);
    walk_to(&mut h, a, APART);
    let from_a = h.bots[a].inbox.len();
    let (b, idb) = h.player_in_world("two", "Babyfare");
    scene_drawn(&mut h, b);
    h.run(20);
    assert!(
        !created(&h, b, 0, ida),
        "not made where the newcomer's cell does not see it"
    );
    assert!(
        !created(&h, a, from_a, idb),
        "nor the newcomer for the other"
    );
    walk_to(&mut h, a, NEXT);
    assert!(
        h.run_until(50, |h| created(h, b, 0, ida)),
        "made as it walks into a cell the newcomer's sees"
    );
    assert!(
        h.run_until(50, |h| created(h, a, from_a, idb)),
        "and the newcomer for it, from the cell it walked into"
    );
}

/// Indoors, an object of the world in the player's own cell is made as the player enters; one in
/// a cell the player's cell sees, once the client has drawn its scene; one in a cell it does not
/// see, only as the player walks into a cell that does; and walking away and back makes nothing
/// again.
/// Behaviour: server.presence.interior-view-by-visible-cells
#[test]
fn objects_in_an_interior_are_made_as_the_players_cell_comes_to_see_theirs() {
    let (here, next, far) = (
        0x4002_9900_0000_0001,
        0x4002_9900_0000_0002,
        0x4002_9900_0000_0003,
    );
    let mut h = Harness::new();
    h.server.ground = Some(Box::new(hall_ground()));
    h.server.content = ContentPack {
        objects: vec![
            static_object(here, ENTRY),
            static_object(next, NEXT),
            static_object(far, FAR),
        ],
        ..ContentPack::default()
    };
    let (b, _) = h.player_in_world("one", "Elara");
    h.run(10);
    assert!(created(&h, b, 0, here), "its own cell's at once");
    assert!(
        !created(&h, b, 0, next),
        "not one in another cell before the scene is drawn"
    );
    scene_drawn(&mut h, b);
    assert!(created(&h, b, 0, next), "the seen cell's once it is");
    assert!(!created(&h, b, 0, far), "not one its cell does not see");
    walk_to(&mut h, b, NEXT);
    assert!(created(&h, b, 0, far), "made as its new cell sees it");
    let back = h.bots[b].inbox.len();
    walk_to(&mut h, b, ENTRY);
    walk_to(&mut h, b, NEXT);
    assert!(
        ![here, next, far].iter().any(|id| created(&h, b, back, *id)),
        "kept: not made again within the place"
    );
}
