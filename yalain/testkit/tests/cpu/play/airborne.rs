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
