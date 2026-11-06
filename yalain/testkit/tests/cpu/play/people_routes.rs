//! Placed people who walk a route: a person whose row names a route walks it as a patrolling
//! creature does, resting at each waypoint and turning back at the end, its place told to the
//! clients that see it and a client new to it making it where it stands now.
//! Fixture: the test hall with one placed person a step from the start, on a route of two
//! waypoints along the hall.

use auberean_protocol::ServerMessage;
use yalain_content::movement::{Gait, Shape, Waypoint};
use yalain_content::{CampMoves, ContentObject, ContentPack, Description, Facts, Kind, Route};
use yalain_testkit::Harness;
use yalain_world::presence;

const CELL: u32 = 0x0299_0109;
const A: [f32; 3] = [60.0, 239.0, -44.6];
const B: [f32; 3] = [60.0, 249.0, -44.6];

/// The hall with a person at `A` walking to `B` and back, resting 4 s at `A` and 3 s at `B`.
fn hall() -> Harness {
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: None,
            template: 0x4700_3183,
            description: Description {
                name_text: None,
                package: 0x103E,
                look: 0x1F00_0100,
                enter_fx: 0,
                scale: None,
                name: Some((0xAE3C_41DB, 0x2500_1B44)),
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: CELL,
            origin: A,
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Npc,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: Facts::default(),
            walks: Some(CampMoves {
                route: Some(Route {
                    key: "pacing".into(),
                    shape: Shape::BackAndForth,
                    gait: Gait::Walk,
                    speed: None,
                    waypoints: vec![
                        Waypoint {
                            cell: CELL,
                            at: A,
                            rest: 4.0,
                        },
                        Waypoint {
                            cell: CELL,
                            at: B,
                            rest: 3.0,
                        },
                    ],
                }),
                start: 1,
                ..CampMoves::default()
            }),
            gate: None,
        }],
        ..ContentPack::default()
    };
    h
}

fn person(h: &Harness) -> (u64, [f32; 3]) {
    (
        presence::content_id(&h.server.content.objects, 0),
        h.server.content.objects[0].origin,
    )
}

fn gap(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn steps(seconds: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (seconds / yalain_testkit::STEP).round() as usize;
    n
}

/// A person on a route walks to the waypoint it sets out toward, rests there as long as the
/// waypoint says, and walks back to the other end, where it rests again before setting out.
/// Behaviour: server.places.person-walks-its-route
#[test]
fn a_placed_person_with_a_route_walks_it_resting_at_each_end_and_turning_back() {
    let mut h = hall();
    let (_b, _) = h.player_in_world("one", "Elara");
    assert!(
        h.run_until(steps(20.0), |h| gap(person(h).1, B) < 0.5),
        "walks to the far end"
    );
    // It rests there 3 s: still at the far end after 2 s.
    h.run(steps(2.0));
    assert!(gap(person(&h).1, B) < 0.5, "rests at the far end");
    assert!(
        h.run_until(steps(20.0), |h| gap(person(h).1, A) < 0.5),
        "walks back to the near end"
    );
    h.run(steps(3.0));
    assert!(gap(person(&h).1, A) < 0.5, "rests at the near end");
    assert!(
        h.run_until(steps(10.0), |h| gap(person(h).1, A) > 1.0),
        "sets out again"
    );
}

/// A walking person's places reach the client that sees it; a client that comes to see it later
/// is made it where it stands then, with the stamp of its last told place.
/// Behaviour: server.places.person-walks-its-route
#[test]
fn a_walking_persons_places_are_told_and_a_newcomer_makes_it_where_it_stands() {
    let mut h = hall();
    let (a, _) = h.player_in_world("one", "Elara");
    let fa = h.bots[a].inbox.len();
    h.run(steps(3.0));
    let (id, now) = person(&h);
    let moved = h.world_messages(a, fa).iter().any(|m| {
        matches!(m, ServerMessage::Position(p) | ServerMessage::PositionCell(p)
            if p.sender.id == id)
    });
    assert!(moved, "the person's places told");
    assert!(gap(now, A) > 1.0, "it has walked");
    let (b, _) = h.player_in_world("two", "Tamsin");
    let made = h.world_messages(b, 0).iter().find_map(|m| match m {
        ServerMessage::CreateObject(o) if o.id == id => Some(o.clone()),
        _ => None,
    });
    let made = made.expect("made for the newcomer");
    assert!(made.physics.stamps[0] > 1, "with its last place's stamp");
    let at = made.physics.position.frame.origin;
    assert!(
        (at.y - h.server.content.objects[0].origin[1]).abs() < 2.0,
        "where it stands, not where it was placed"
    );
}
