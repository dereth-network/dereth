//! What a client reports of its player's place: taken within what a body can do; beyond it the player
//! stays, and its client is told where.

use auberean_primitives::Vec3;
use auberean_protocol::{queue, ServerMessage};
use yalain_testkit::play::report;
use yalain_testkit::Harness;
use yalain_world::world::{land_cell_holds, Travel, TRAVEL_HELD, TRAVEL_SPEED};

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
    let m = report(
        far,
        Vec3::new(86.0, -13.0, 32.0),
        0,
        yalain_world::world::IDLE_MOTION,
        0,
    );
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
    let stale = report(cell, near, 0, yalain_world::world::IDLE_MOTION, 0);
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
    let mut m = report(cell, near, 0, yalain_world::world::IDLE_MOTION, 0);
    if let auberean_protocol::ClientMessage::Position(p) = &mut m {
        p.force_position_stamp = 1;
    }
    h.bots[a].send(true, queue::WEENIE, &m);
    h.run(5);
    assert_ne!(h.server.players[&world].position.origin, before.origin);
}

/// A body may keep going for as long as it likes at speeds a body has, and what a report spends comes
/// back with time.
/// Behaviour: server.places.a-place-too-far-is-told-back
#[test]
fn what_a_player_may_cover_comes_back_with_time() {
    let mut t = Travel::default();
    assert!(t.spend(TRAVEL_HELD, 0.0), "at once, as far as it holds");
    assert!(!t.spend(1.0, 0.0), "and no further");
    assert!(t.spend(TRAVEL_SPEED, 1.0), "a second's worth a second on");
    assert!(!t.spend(1.0, 1.0));
    let mut walking = Travel::default();
    for i in 0..100_000 {
        let now = f64::from(i) * 0.2;
        assert!(walking.spend(40.0 * 0.2, now), "at 40 m/s, report {i}");
    }
    // A long rest gives no more than it holds.
    let mut rested = Travel::default();
    assert!(rested.spend(0.0, 0.0));
    assert!(!rested.spend(TRAVEL_HELD + 1.0, 1.0e6));
    // Nor does a clock that goes back take any.
    assert!(rested.spend(TRAVEL_HELD, 5.0));
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
    let m = report(
        BLOCK | 0x41,
        Vec3::new(x, y, 0.0),
        0,
        yalain_world::world::IDLE_MOTION,
        0,
    );
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
    let m = report(
        far,
        Vec3::new(86.0, -13.0, 32.0),
        0,
        yalain_world::world::IDLE_MOTION,
        0,
    );
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
