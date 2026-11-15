//! An effect that pushes a player: the velocity of its ways and speed, and its telling.

use auberean_protocol::ServerMessage;
use yalain_testkit::{Harness, DASH_EFFECT};
use yalain_world::effect_defs::{impulse_way as way, EffectDef, Kind, Scale};
use yalain_world::effects::{self, impulse_velocity, IMPULSE, IMPULSE_TOP_SPEED};

const PUSH: u32 = 0x6F00_A101;

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
}

/// Away from the caster the push follows the offset from caster to target in three dimensions;
/// up it is straight up; either at its speed, never above 50 m/s. Up and against the target's
/// facing goes halfway between; the ways an effect names add up; a push from a caster standing
/// where the target stands, or with no way, goes nowhere. Back is taken as against the target's
/// own facing, and forward along it: no recorded push settles which facing they mean, and only the
/// effects that name them (the rally's fling among them) depend on it.
/// Behaviour: server.effects.impulse-velocity
#[test]
fn an_impulse_goes_its_ways_at_its_speed_and_no_faster_than_fifty() {
    let away = Some([3.17, 0.31, -0.43]);
    let len = (3.17f32 * 3.17 + 0.31 * 0.31 + 0.43 * 0.43).sqrt();
    let expect = [3.17 / len * 50.0, 0.31 / len * 50.0, -0.43 / len * 50.0];
    let v = impulse_velocity(way::FROM_CASTER, 1000.0, 0.0, away).expect("pushed");
    assert!(close(v, expect), "{v:?}");
    let v = impulse_velocity(way::FROM_CASTER, 15.0, 0.0, away).expect("pushed");
    assert!((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() - 15.0 < 1e-3);
    assert!(close(
        impulse_velocity(way::UP, 10.0, 123.0, None).unwrap(),
        [0.0, 0.0, 10.0]
    ));
    // Facing east (90 degrees): back is west.
    let h = 50.0 / 2f32.sqrt();
    assert!(close(
        impulse_velocity(way::UP_AND_BACK, 66.0, 90.0, None).unwrap(),
        [-h, 0.0, h]
    ));
    assert!(close(
        impulse_velocity(way::UP | way::BACKWARD, 66.0, 90.0, None).unwrap(),
        [-h, 0.0, h]
    ));
    assert!(close(
        impulse_velocity(way::FORWARD, 20.0, 0.0, None).unwrap(),
        [0.0, 20.0, 0.0]
    ));
    assert_eq!(
        impulse_velocity(way::FROM_CASTER, 50.0, 0.0, Some([0.0; 3])),
        None
    );
    assert_eq!(impulse_velocity(way::FROM_CASTER, 50.0, 0.0, None), None);
    assert_eq!(impulse_velocity(0, 50.0, 0.0, None), None);
    assert!((IMPULSE_TOP_SPEED - 50.0).abs() < f32::EPSILON);
}

/// A player pushed away from another: its client and the other's are told it flies from where it
/// stands, a position with the impulse and its contact, at the impulse's velocity away from the
/// caster, its position stamp moved on.
/// Behaviour: server.effects.impulse-pushes-a-player
#[test]
fn a_pushed_player_is_told_its_flight_and_so_is_who_sees_it() {
    let mut h = Harness::new();
    let (a, me) = h.player_in_world("one", "Elara");
    let (b, caster) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let dash = h
        .server
        .effect_table
        .get(DASH_EFFECT)
        .cloned()
        .expect("dash");
    h.server.effect_table.effects.insert(
        PUSH,
        EffectDef {
            state: PUSH,
            kind: Kind::Impulse {
                speed: Scale(vec![(1.0, 1000.0)]),
                ways: way::FROM_CASTER,
            },
            duration: Scale::default(),
            eq_class: 0,
            ..dash
        },
    );
    let world_of = |h: &Harness, id: u64| {
        *h.server
            .players
            .iter()
            .find(|(_, p)| p.character == id)
            .expect("in the world")
            .0
    };
    let (mine, theirs) = (world_of(&h, me), world_of(&h, caster));
    let mut at = h.server.players[&theirs].position;
    at.origin[0] -= 3.0;
    at.origin[2] -= 0.5;
    h.server.players.get_mut(&theirs).unwrap().position = at;
    let target = h.server.players[&mine].position;
    let stamp = h.server.players[&mine].position_stamp;
    let (from_a, from_b) = (h.bots[a].inbox.len(), h.bots[b].inbox.len());
    let cast = effects::Cast {
        caster,
        spellcraft: 1.0,
        pk: true,
        ..effects::Cast::default()
    };
    let clock = h.clock();
    effects::apply(&mut h.server, &mut h.net, mine, PUSH, cast, clock);
    h.run(4);
    let (dx, dy, dz) = yalain_world::ai::offset(&at, &target);
    let want = impulse_velocity(way::FROM_CASTER, 1000.0, 0.0, Some([dx, dy, dz])).unwrap();
    for (bot, from) in [(a, from_a), (b, from_b)] {
        let flights: Vec<([f32; 3], u32, u16)> = h
            .world_messages(bot, from)
            .iter()
            .filter_map(|m| match m {
                ServerMessage::Position(p) if p.sender.id == me && p.flags & IMPULSE != 0 => {
                    Some((
                        [p.velocity.x, p.velocity.y, p.velocity.z],
                        p.flags,
                        p.position_stamp,
                    ))
                }
                _ => None,
            })
            .collect();
        assert_eq!(flights.len(), 1, "told once");
        assert!(close(flights[0].0, want), "{:?} for {want:?}", flights[0].0);
        assert_eq!(flights[0].1, 0xC, "the impulse and contact");
        assert_eq!(flights[0].2, stamp.wrapping_add(1));
    }
    assert!(
        h.server.players[&mine]
            .play
            .effects
            .iter()
            .all(|e| e.state != PUSH),
        "no record kept"
    );
}
