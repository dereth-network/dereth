//! In the air: no skill is used from a reported jump until it comes back down, nor while the
//! places reported after it keep going down; once a report goes down no further, skills are used
//! again.

use auberean_primitives::Vec3;
use auberean_protocol::physics::position_flags;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_testkit::play::{report, server_call};
use yalain_testkit::{Harness, INNER_CALM};
use yalain_world::calls::{self, Value};
use yalain_world::player::quality;
use yalain_world::skill_use::{function, refusal};

const SWORD: u32 = 0x4700_051C;

/// A player armed and with vigor enough for Inner Calm; returns the client and its connection.
fn armed() -> (Harness, usize, u64, ConnId) {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    h.arm(b, me, SWORD);
    let world = *h
        .server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0;
    let p = h.server.players.get_mut(&world).expect("in the world");
    yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
    yalain_world::quals::set_int(&mut p.qualities, quality::VIGOR, 400);
    (h, b, me, world)
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// The client reports its player `dz` m above where it stands now, jumping upward at `jump` m/s
/// when given.
fn reports(h: &mut Harness, b: usize, world: ConnId, dz: f32, jump: Option<f32>) {
    let p = &h.server.players[&world];
    let at = p.position;
    let mut m = report(
        at.cell,
        Vec3::new(at.origin[0], at.origin[1], at.origin[2] + dz),
        0,
        yalain_world::world::IDLE_MOTION,
        p.teleport_stamp,
    );
    if let (ClientMessage::Position(c), Some(up)) = (&mut m, jump) {
        c.flags |= position_flags::JUMP;
        c.jump_velocity = Vec3::new(0.0, 0.0, up);
    }
    h.bots[b].send(true, queue::WEENIE, &m);
    h.run(2);
}

/// Use Inner Calm on the player; the refusal's code, if one came.
fn refusal_of(h: &mut Harness, b: usize, id: u32, me: u64) -> Option<u32> {
    h.all_hold(INNER_CALM);
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(
            function::USE,
            &[Arg::Int(id), Arg::Int(INNER_CALM), Arg::LongInt(me)],
        ),
    );
    h.run(4);
    let got = h.world_messages(b, from).iter().find_map(|m| match m {
        ServerMessage::PlayerCall(c) if c.function.0 == function::REFUSED => c
            .args
            .as_ref()
            .and_then(|a| calls::values(&a.0).ok())
            .and_then(|v| match v.last() {
                Some(Value::Int(code)) => Some(*code),
                _ => None,
            }),
        _ => None,
    });
    got
}

/// A jump upward at 4.9 m/s comes back down a second later: a use asked at once is refused as in
/// the air; one asked after that second, with no report since, is carried out.
/// Behaviour: server.skills.not-in-the-air
#[test]
fn no_skill_is_used_in_the_air_until_the_jump_comes_down() {
    let (mut h, b, me, world) = armed();
    reports(&mut h, b, world, 0.0, Some(4.9));
    assert_eq!(refusal_of(&mut h, b, 1, me), Some(refusal::IN_THE_AIR));
    run_for(&mut h, 0.4);
    assert_eq!(refusal_of(&mut h, b, 2, me), Some(refusal::IN_THE_AIR));
    run_for(&mut h, 0.8);
    assert_eq!(refusal_of(&mut h, b, 3, me), None);
}

/// Off a ledge: past a short jump's own time, reports still going down keep the player in the
/// air; the first that goes down no further lands it, and a use is carried out, even as later
/// reports go down a slope.
/// Behaviour: server.skills.not-in-the-air
#[test]
fn a_jump_off_a_ledge_is_in_the_air_while_the_reports_keep_going_down() {
    let (mut h, b, me, world) = armed();
    reports(&mut h, b, world, 0.0, Some(2.0));
    run_for(&mut h, 0.6);
    reports(&mut h, b, world, -1.0, None);
    assert_eq!(refusal_of(&mut h, b, 1, me), Some(refusal::IN_THE_AIR));
    run_for(&mut h, 0.3);
    reports(&mut h, b, world, -2.0, None);
    assert_eq!(refusal_of(&mut h, b, 2, me), Some(refusal::IN_THE_AIR));
    reports(&mut h, b, world, 0.0, None);
    // Landed: walking down a slope after it is no fall.
    reports(&mut h, b, world, -0.5, None);
    assert_eq!(refusal_of(&mut h, b, 3, me), None);
}

/// The client reports its player `dz` m lower than where it stands, `dt` seconds after its last
/// report, with no jump.
fn drops(h: &mut Harness, b: usize, world: ConnId, dz: f32, dt: f64) {
    run_for(h, dt);
    let p = &h.server.players[&world];
    let at = p.position;
    let mut m = report(
        at.cell,
        Vec3::new(at.origin[0], at.origin[1], at.origin[2] - dz),
        0,
        yalain_world::world::IDLE_MOTION,
        p.teleport_stamp,
    );
    if let ClientMessage::Position(c) = &mut m {
        c.time = h.now.0;
    }
    h.bots[b].send(true, queue::WEENIE, &m);
    h.run(1);
}

/// Down a slope at a walk, a use is carried out. Off a ledge with no jump: once the reports go down
/// as a fall does (2 m in 0.2 s, straight down),
/// a use is refused as in the air; the first report that goes down no further lands the player,
/// and no use is refused as in the air.
/// Behaviour: server.skills.not-in-the-air
#[test]
fn a_fall_off_a_ledge_with_no_jump_is_in_the_air() {
    let (mut h, b, me, world) = armed();
    // Down a slope at a walk (1 m/s) is no fall.
    drops(&mut h, b, world, 0.0, 0.2);
    drops(&mut h, b, world, 0.2, 0.2);
    assert_eq!(refusal_of(&mut h, b, 1, me), None);
    run_for(&mut h, 4.0);
    drops(&mut h, b, world, 0.0, 0.2);
    drops(&mut h, b, world, 2.0, 0.2);
    assert_eq!(refusal_of(&mut h, b, 2, me), Some(refusal::IN_THE_AIR));
    drops(&mut h, b, world, 0.0, 0.2);
    // Landed: no longer in the air (the skill may still be recovering from its first use).
    assert_ne!(refusal_of(&mut h, b, 3, me), Some(refusal::IN_THE_AIR));
}

/// The server pushes the player once, its ways at `speed` m/s (an impulse effect cast on it by
/// itself).
fn pushed(h: &mut Harness, world: ConnId, me: u64, ways: u32, speed: f32) {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    const PUSH: u32 = 0x6F00_A101;
    let dash = h
        .server
        .effect_table
        .get(yalain_testkit::DASH_EFFECT)
        .cloned()
        .expect("dash");
    h.server.effect_table.effects.insert(
        PUSH,
        EffectDef {
            state: PUSH,
            kind: Kind::Impulse {
                speed: Scale(vec![(1.0, speed)]),
                ways,
            },
            duration: Scale::default(),
            eq_class: 0,
            ..dash
        },
    );
    let cast = yalain_world::effects::Cast {
        caster: me,
        spellcraft: 1.0,
        ..yalain_world::effects::Cast::default()
    };
    let clock = h.clock();
    yalain_world::effects::apply(&mut h.server, &mut h.net, world, PUSH, cast, clock);
    h.run(1);
}

/// A push straight up at 4.9 m/s puts the player in the air as a jump at that speed does (2017:
/// every use asked within an upward push's time aloft was refused, 5 of 5, 4 of them as in the
/// air): a use asked at once is refused as in the air, one asked after its second is carried out.
/// A push along the ground leaves the player on it.
/// Behaviour: server.skills.not-in-the-air
#[test]
fn a_push_upward_is_in_the_air_as_a_jump_is() {
    use yalain_world::effect_defs::impulse_way as way;
    let (mut h, b, me, world) = armed();
    pushed(&mut h, world, me, way::UP, 4.9);
    assert_eq!(refusal_of(&mut h, b, 1, me), Some(refusal::IN_THE_AIR));
    run_for(&mut h, 1.2);
    assert_eq!(refusal_of(&mut h, b, 2, me), None);
    pushed(&mut h, world, me, way::FORWARD, 20.0);
    // On the ground (the skill may still be recovering from its last use).
    assert_ne!(refusal_of(&mut h, b, 3, me), Some(refusal::IN_THE_AIR));
}
