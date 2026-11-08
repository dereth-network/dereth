//! What a client reports of its player's place: taken within what a body can do (its reach in the
//! time since the last, steady going, the rooms' doorways and sight, the land); beyond it the
//! player stays, and its client is told where.

use auberean_primitives::Vec3;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind, Place};
use yalain_store::StoredPosition;
use yalain_testkit::play::{report, server_call, use_of};
use yalain_testkit::{Harness, TestGround};
use yalain_world::reported::{
    self, Refused, Travel, AT_ONCE, IN_A_WINDOW, OUTDOORS, SPEED, UNDER_THE_LAND,
};
use yalain_world::world::{land_cell_holds, IDLE_MOTION};

const BLOCK: u32 = 0x0298_0000;

/// Two players in one place, whose reports are held; the second watching.
fn two() -> (Harness, usize, u64, usize, yalain_net::ConnId) {
    let mut h = Harness::new();
    h.server.cfg.holds_reports = true;
    let (a, id) = h.player_in_world("one", "Elara");
    let (b, _) = h.player_in_world("two", "Bran");
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0;
    (h, a, id, b, world)
}

/// A place in the first player's cell, this far along.
fn along(h: &Harness, world: yalain_net::ConnId, meters: f32) -> (u32, Vec3) {
    let at = h.server.players[&world].position;
    (
        at.cell,
        Vec3::new(at.origin[0] + meters, at.origin[1], at.origin[2]),
    )
}

/// What a client has been told of the first player's place, from its nth world message on.
fn told_place(h: &Harness, a: usize, id: u64, from: usize) -> Vec<(u32, [f32; 3], u16)> {
    h.world_messages(a, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::Position(p) | ServerMessage::PositionCell(p) if p.sender.id == id => {
                Some((
                    p.cell,
                    [p.origin.x, p.origin.y, p.origin.z],
                    p.force_position_stamp,
                ))
            }
            _ => None,
        })
        .collect()
}

/// A place further than a body could have gone leaves the player where it stood: its client is told
/// that place with a newer force-position stamp, nobody else hears of it, and what the client says
/// next counts only once it has heard.
/// Behaviour: server.places.a-place-too-far-is-told-back
#[test]
fn a_place_too_far_leaves_the_player_where_it_stood_and_its_client_is_told() {
    let (mut h, a, id, b, world) = two();
    h.run(5);
    let (from_a, from_b) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    let before = h.server.players[&world].position;
    let far = 0x03E7_0192;
    let m = report(far, Vec3::new(86.0, -13.0, 32.0), 0, IDLE_MOTION, 0);
    h.bots[a].send(true, queue::WEENIE, &m);
    h.run(5);
    assert_eq!(h.server.players[&world].position.cell, before.cell);
    assert_eq!(h.server.players[&world].position.origin, before.origin);
    assert_eq!(h.server.players[&world].force_position_stamp, 1);
    assert_eq!(
        told_place(&h, a, id, from_a),
        vec![(before.cell, before.origin, 1)]
    );
    assert_eq!(
        told_place(&h, b, id, from_b),
        vec![],
        "the other hears nothing"
    );
    // One from before it heard, near as it is, changes nothing; in a while it is told again.
    let from_a = h.bots[a].inbox.len();
    let (cell, near) = along(&h, world, 1.0);
    let stale = report(cell, near, 0, IDLE_MOTION, 0);
    h.bots[a].send(true, queue::WEENIE, &stale);
    h.run(5);
    assert_eq!(h.server.players[&world].position.origin, before.origin);
    assert_eq!(told_place(&h, a, id, from_a), vec![], "not at once");
    h.step_by(yalain_world::world::TOLD_BACK_AGAIN);
    h.bots[a].send(true, queue::WEENIE, &stale);
    h.run(5);
    assert_eq!(
        told_place(&h, a, id, from_a),
        vec![(before.cell, before.origin, 1)]
    );
    // One that has heard is taken.
    let mut m = report(cell, near, 0, IDLE_MOTION, 0);
    if let auberean_protocol::ClientMessage::Position(p) = &mut m {
        p.force_position_stamp = 1;
    }
    h.bots[a].send(true, queue::WEENIE, &m);
    h.run(5);
    assert_ne!(h.server.players[&world].position.origin, before.origin);
}

/// A land cell is the one its point lies in; a report of any other is no place, and the player stays.
/// Behaviour: server.places.a-land-cell-holds-its-point
#[test]
fn a_land_cell_its_point_does_not_lie_in_is_no_place() {
    assert!(land_cell_holds(BLOCK | 1, [0.0, 0.0]));
    assert!(land_cell_holds(BLOCK | 2, [0.0, 20.0]));
    assert!(land_cell_holds(BLOCK | 9, [20.0, 0.0]));
    assert!(land_cell_holds(BLOCK | 64, [159.9, 159.9]));
    assert!(land_cell_holds(BLOCK | 64, [160.0, 160.0]));
    assert!(!land_cell_holds(BLOCK | 1, [20.0, 0.0]));
    assert!(!land_cell_holds(BLOCK | 65, [159.9, 159.9]));
    assert!(!land_cell_holds(BLOCK, [0.0, 0.0]));
    assert!(!land_cell_holds(BLOCK | 1, [-0.1, 0.0]));
    assert!(!land_cell_holds(BLOCK | 64, [160.1, 0.0]));

    let (mut h, a, _, _, world) = two();
    h.run(5);
    let before = h.server.players[&world].position;
    let (x, y) = (before.origin[0], before.origin[1]);
    let m = report(BLOCK | 0x41, Vec3::new(x, y, 0.0), 0, IDLE_MOTION, 0);
    h.bots[a].send(true, queue::WEENIE, &m);
    h.run(5);
    assert_eq!(h.server.players[&world].position.cell, before.cell);
    assert_eq!(h.server.players[&world].force_position_stamp, 0);
}

/// An admin's character goes where it likes: its commands may give it any speed.
/// Behaviour: server.places.a-place-too-far-is-told-back
#[test]
fn an_admin_characters_reports_are_not_held() {
    let (mut h, a, _, _, world) = two();
    h.run(5);
    h.server
        .players
        .get_mut(&world)
        .expect("in the world")
        .admin_character = true;
    let far = 0x03E7_0192;
    let m = report(far, Vec3::new(86.0, -13.0, 32.0), 0, IDLE_MOTION, 0);
    h.bots[a].send(true, queue::WEENIE, &m);
    h.run(5);
    assert_eq!(h.server.players[&world].position.cell, far);
}

/// A world holds reports unless it is told otherwise.
/// Behaviour: server.places.a-place-too-far-is-told-back
#[test]
fn a_world_holds_reports_by_default() {
    assert!(yalain_world::ServerConfig::default().holds_reports);
}

/// The first player's place.
fn at(h: &Harness, world: yalain_net::ConnId) -> StoredPosition {
    h.server.players[&world].position
}

/// A report of a place in a block's frame, its land cell worked out from its point outdoors, with
/// the stamps given.
fn place_report(cell: u32, origin: [f32; 3], teleport: u16, force: u16) -> ClientMessage {
    let mut m = report(
        cell,
        Vec3::new(origin[0], origin[1], origin[2]),
        0,
        IDLE_MOTION,
        teleport,
    );
    if let ClientMessage::Position(p) = &mut m {
        p.force_position_stamp = force;
    }
    m
}

/// The land cell of a point of block `block` (`cell >> 16`), and the point within it.
fn outdoors(block: u32, x: f32, y: f32, z: f32) -> (u32, [f32; 3]) {
    let (cell, x, y) = yalain_world::spawn::outdoor(block, x, y);
    (cell, [x, y, z])
}

/// Two players, the first moved out to a place outdoors in the test block, the clock on a while.
fn outside() -> (
    Harness,
    usize,
    u64,
    usize,
    yalain_net::ConnId,
    StoredPosition,
) {
    let (mut h, a, id, b, world) = two();
    h.run(5);
    let (cell, origin) = outdoors(BLOCK >> 16, 80.0, 80.0, 30.0);
    let p = h.server.players.get_mut(&world).expect("in the world");
    p.position = StoredPosition {
        cell,
        origin,
        rotation: [1.0, 0.0, 0.0, 0.0],
    };
    p.travel = Travel::placed(h.now.0);
    let start = p.position;
    (h, a, id, b, world, start)
}

/// Send a report, let it arrive and what it brings reach the client.
fn send(h: &mut Harness, a: usize, m: &ClientMessage) {
    h.bots[a].send(true, queue::WEENIE, m);
    h.run(4);
}

/// A report reaches 30 m however soon it follows the last place taken, and 15 m a second more by
/// the time between their arrivals; one further is not taken.
/// Behaviour: server.places.a-report-is-within-reach-of-its-time
#[test]
fn a_report_reaches_thirty_meters_and_fifteen_more_a_second_since_the_last() {
    let t = Travel::placed(100.0);
    assert_eq!(t.reaches(AT_ONCE - 0.01, 100.0), Ok(()));
    assert_eq!(t.reaches(AT_ONCE + 0.01, 100.0), Err(Refused::Reach));
    assert_eq!(t.reaches(AT_ONCE + 2.0 * SPEED - 0.01, 102.0), Ok(()));
    assert_eq!(
        t.reaches(AT_ONCE + 2.0 * SPEED + 0.01, 102.0),
        Err(Refused::Reach)
    );
    // A long silence reaches far, as a recorded 1,422 m after 252 s did.
    assert_eq!(t.reaches(1_422.0, 352.0), Ok(()));
    // A clock that goes back gives no more than at once.
    assert_eq!(t.reaches(AT_ONCE + 0.01, 50.0), Err(Refused::Reach));

    let (mut h, a, id, b, world, start) = outside();
    let (from_a, from_b) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    // Arriving within two steps of now: at most this far.
    #[allow(clippy::cast_possible_truncation)]
    let soon = AT_ONCE + SPEED * (h.now.0 + 0.2 - h.server.players[&world].travel.since()) as f32;
    let (cell, o) = outdoors(BLOCK >> 16, 80.0, 80.0 + soon + 2.0, 30.0);
    send(&mut h, a, &place_report(cell, o, 0, 0));
    assert_eq!(at(&h, world).origin, start.origin, "not taken");
    assert_eq!(h.server.players[&world].force_position_stamp, 1);
    assert_eq!(
        told_place(&h, a, id, from_a),
        vec![(start.cell, start.origin, 1)]
    );
    assert_eq!(
        told_place(&h, b, id, from_b),
        vec![],
        "the other hears nothing"
    );
    let (cell, o) = outdoors(BLOCK >> 16, 80.0, 80.0 + AT_ONCE - 2.0, 30.0);
    send(&mut h, a, &place_report(cell, o, 0, 1));
    assert_eq!(at(&h, world).origin, o, "taken");
}

/// Reports each within a second of the last cover at most 400 m in any 10 s; one that would go
/// further is not taken, and going steady just under it goes on for as long as it likes.
/// Behaviour: server.places.steady-going-is-held-to-a-window
#[test]
fn steady_reports_cover_at_most_four_hundred_meters_in_ten_seconds() {
    // Every half second: 20 steps in a window.
    let steady = |meters: f32, n: u32| -> Option<u32> {
        let mut t = Travel::placed(0.0);
        for k in 1..=n {
            let now = f64::from(k) * 0.5;
            if t.reaches(meters, now).is_err() {
                return Some(k);
            }
            t.take(meters, now);
        }
        None
    };
    let most = IN_A_WINDOW / 20.0;
    assert_eq!(steady(most - 0.1, 2_000), None);
    assert_eq!(steady(most + 0.1, 2_000), Some(20));
    // A pause of over a second starts the going again.
    let mut t = Travel::placed(0.0);
    for k in 1..=19 {
        t.take(most + 0.1, f64::from(k) * 0.5);
    }
    assert_eq!(t.reaches(most + 0.1, 10.0), Err(Refused::Steady));
    assert_eq!(t.reaches(most + 0.1, 10.6), Ok(()));
    // And the going before the pause no longer counts after it.
    let mut t = Travel::placed(0.0);
    for k in 1..=10 {
        t.take(37.0, f64::from(k) * 0.5);
    }
    t.take(37.0, 6.1);
    assert_eq!(t.reaches(37.0, 6.6), Ok(()));

    // On the wire: back and forth along the land, a report every half second.
    let run = |meters: f32| -> (Option<usize>, u16) {
        let (mut h, a, _, _, world, start) = outside();
        let mut refused = None;
        for k in 0..30 {
            let y = if k % 2 == 0 { 80.0 + meters } else { 80.0 };
            let (cell, o) = outdoors(BLOCK >> 16, 80.0, y, start.origin[2]);
            h.bots[a].send(true, queue::WEENIE, &place_report(cell, o, 0, 0));
            h.step_by(0.25);
            h.step_by(0.25);
            if refused.is_none() && h.server.players[&world].force_position_stamp != 0 {
                refused = Some(k);
            }
        }
        (refused, h.server.players[&world].force_position_stamp)
    };
    assert_eq!(run(18.0), (None, 0));
    let (refused, stamp) = run(22.0);
    assert!(refused.is_some_and(|k| k >= 17), "{refused:?}");
    assert_eq!(stamp, 1);
}

/// A report into a room comes through a doorway of the room it left or one that room sees, and from
/// outdoors only into a room with a doorway to the outdoors; a room the world's records lack is let
/// in and out of either way.
/// Behaviour: server.places.a-room-is-entered-by-a-doorway-or-sight
#[test]
fn a_room_is_entered_only_through_a_doorway_or_a_room_seen() {
    let room = |low: u32| BLOCK | low;
    let g = TestGround::default()
        .with_room(room(0x100), &[OUTDOORS, room(0x101)], &[room(0x101)])
        .with_room(room(0x101), &[room(0x100)], &[room(0x100), room(0x102)])
        .with_room(room(0x102), &[room(0x103)], &[])
        .with_room(room(0x103), &[], &[]);
    let ground: Option<&dyn yalain_world::Ground> = Some(&g);
    let (out, _) = outdoors(BLOCK >> 16, 80.0, 80.0, 0.0);
    assert!(reported::way_in(ground, out, room(0x100)));
    assert!(
        !reported::way_in(ground, out, room(0x101)),
        "no doorway out"
    );
    assert!(reported::way_in(ground, room(0x100), room(0x101)));
    assert!(reported::way_in(ground, room(0x101), room(0x100)));
    assert!(reported::way_in(ground, room(0x101), room(0x102)), "seen");
    assert!(
        reported::way_in(ground, room(0x103), room(0x102)),
        "joined from its side"
    );
    assert!(
        !reported::way_in(ground, room(0x102), room(0x101)),
        "seen only from there"
    );
    assert!(
        !reported::way_in(ground, room(0x100), room(0x103)),
        "a wall"
    );
    assert!(
        reported::way_in(ground, room(0x100), room(0x150)),
        "a room not held"
    );
    assert!(reported::way_in(ground, room(0x150), room(0x103)));
    assert!(reported::way_in(ground, out, room(0x150)));
    assert!(
        reported::way_in(ground, room(0x103), out),
        "leaving is not checked"
    );
    assert!(reported::way_in(None, out, room(0x101)), "nothing known");

    let (mut h, a, _, _, world, start) = outside();
    h.server.ground = Some(Box::new(g));
    let o = start.origin;
    let mut force = 0;
    let mut go = |h: &mut Harness, cell: u32| -> bool {
        h.step_by(1.0);
        send(h, a, &place_report(cell, o, 0, force));
        let taken = at(h, world).cell == cell;
        force = h.server.players[&world].force_position_stamp;
        taken
    };
    assert!(!go(&mut h, room(0x101)), "no doorway to the outdoors");
    assert!(go(&mut h, room(0x100)));
    assert!(!go(&mut h, room(0x103)), "through a wall");
    assert!(go(&mut h, room(0x101)));
    assert!(go(&mut h, room(0x102)));
    assert!(go(&mut h, room(0x150)), "a room not held");
    assert!(go(&mut h, room(0x103)));
    assert_eq!(force, 2);
}

/// A report outdoors more than half a meter below the land is not taken, nor one further above it
/// than its time allows; one at the land, or just under it, is.
/// Behaviour: server.places.a-place-outdoors-is-not-under-the-land
#[test]
fn a_place_far_under_the_land_or_further_above_it_than_its_time_allows_is_not_taken() {
    let (mut h, a, _, _, world, start) = outside();
    let land = start.origin[2];
    h.server.ground = Some(Box::new(TestGround {
        land: Some(land),
        ..TestGround::default()
    }));
    let mut force = 0;
    let mut go = |h: &mut Harness, z: f32| -> bool {
        h.step_by(1.0);
        let (cell, p) = outdoors(BLOCK >> 16, 81.0, 81.0, z);
        send(h, a, &place_report(cell, p, 0, force));
        let taken = at(h, world).origin == p;
        force = h.server.players[&world].force_position_stamp;
        taken
    };
    assert!(go(&mut h, land - UNDER_THE_LAND + 0.1));
    assert!(!go(&mut h, land - UNDER_THE_LAND - 0.1), "under the land");
    assert!(!go(&mut h, land - 50.0), "far under it");
    assert!(go(&mut h, land + 40.0), "up as far as a second allows");
    assert!(
        !go(&mut h, land + 40.0 + AT_ONCE + SPEED + 10.0),
        "higher than a second allows"
    );
    assert!(go(&mut h, land));
    assert_eq!(force, 3);
}

const PORTAL: u64 = 0x4002_9900_0000_0061;
const BLUE: u32 = 0x4700_114D;
const AWAY: Place = Place {
    cell: 0x7000_0001,
    origin: [10.0, 10.0, 50.0],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

/// A trip's end puts the player somewhere new, and its client's first report there is taken; the
/// reckoning starts again where and when the trip ended, so the time spent on the way gives no
/// reach. A place the server told back is taken from in the same way.
/// Behaviour: server.places.a-place-given-starts-the-reckoning-again
#[test]
fn a_trips_end_and_a_place_told_back_are_taken_and_the_reckoning_starts_there() {
    let (mut h, a, id, _, world) = two();
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(PORTAL),
            template: BLUE,
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
            origin: [61.0, 240.2, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: Some(AWAY),
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    h.run(10);
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(PORTAL, id)]),
    );
    assert!(h.run_until(400, |h| h.server.players[&world].teleport_stamp == 1));
    assert_eq!(at(&h, world).cell, AWAY.cell);
    // Further from where it came out than a moment allows, however long the trip took.
    let (cell, far) = outdoors(AWAY.cell >> 16, 10.0, 10.0 + AT_ONCE + 5.0, 50.0);
    send(&mut h, a, &place_report(cell, far, 1, 0));
    assert_eq!(at(&h, world).cell, AWAY.cell);
    assert_eq!(at(&h, world).origin, AWAY.origin);
    assert_eq!(h.server.players[&world].force_position_stamp, 1);
    // From the place told back, at its stamp: taken, and going on from there.
    let mut force = 1;
    for k in 1..=20 {
        h.step_by(0.25);
        let (cell, p) = outdoors(AWAY.cell >> 16, 10.0, 10.0 + 3.0 * k as f32, 50.0);
        send(&mut h, a, &place_report(cell, p, 1, force));
        assert_eq!(at(&h, world).origin, p, "report {k}");
        force = h.server.players[&world].force_position_stamp;
    }
    assert_eq!(force, 1);
}
