//! Falls: a player whose client reports it falling is hurt by the height as it lands, mounted or
//! not, spared by Safe Fall, by its fall modifier and by water; a ride down a slope is no fall.
//! Fixture: a player where a character starts (outdoors over made-up ground and water where a test
//! says), its client reporting places a fifth of a second apart.

use auberean_primitives::Vec3;
use auberean_protocol::physics::ClientPosition;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_net::ConnId;
use yalain_store::StoredPosition;
use yalain_testkit::{Harness, TestGround};
use yalain_world::calls::{Node, Value};
use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
use yalain_world::falls;
use yalain_world::player::quality;

/// The gap between two of the client's reports, in seconds.
const STEP: f64 = 0.2;
const DEATH_MESSAGE: u32 = 0x8207_002B;
/// A made-up effect of Safe Fall's type.
const SAFE: u32 = 0x7000_7D01;

/// A player in the world with health to spare: the harness, its client, its world connection and
/// where it stands.
fn player(health: i32) -> (Harness, usize, ConnId, StoredPosition) {
    let mut h = Harness::new();
    let (b, me) = h.player_in_world("one", "Elara");
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(auberean_protocol::login::Empty),
    );
    h.run(5);
    let (world, at) = h
        .server
        .players
        .iter_mut()
        .find(|(_, p)| p.character == me)
        .map(|(w, p)| {
            yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH_MAX, health);
            yalain_world::quals::set_int(&mut p.qualities, quality::HEALTH, health);
            (*w, p.position)
        })
        .expect("in the world");
    (h, b, world, at)
}

/// The client reports its player at `at` (the harness's clock its time); the world runs to its next
/// report.
fn report(h: &mut Harness, b: usize, at: [f32; 3], cell: u32) {
    let time = h.now.0;
    h.bots[b].send(
        true,
        queue::WEENIE,
        &ClientMessage::Position(ClientPosition {
            time,
            cell,
            origin: Vec3::new(at[0], at[1], at[2]),
            heading_motion: yalain_world::world::IDLE_MOTION,
            flags: 4,
            instance_stamp: 1,
            teleport_stamp: 0,
            force_position_stamp: 0,
            move_to_stamp: 0,
            jump_velocity: Vec3::ZERO,
        }),
    );
    let until = h.now.0 + STEP;
    while h.now.0 < until {
        h.step();
    }
}

/// The client reports its player dropping `height` meters from `from`, going across at `across`
/// meters a second, as a body dropping from rest does, then standing where it landed; returns where.
fn drop(
    h: &mut Harness,
    b: usize,
    from: [f32; 3],
    cell: u32,
    height: f32,
    across: f32,
) -> [f32; 3] {
    let [x, y, z] = from;
    report(h, b, from, cell);
    let t0 = h.now.0;
    let mut at = from;
    while at[2] > z - height {
        #[allow(clippy::cast_possible_truncation)]
        let s = (h.now.0 - t0) as f32;
        at = [x + across * s, y, z - (4.9 * s * s).min(height)];
        report(h, b, at, cell);
    }
    report(h, b, at, cell);
    report(h, b, at, cell);
    at
}

fn health(h: &Harness, world: ConnId) -> i32 {
    yalain_world::quals::int(&h.server.players[&world].qualities, quality::HEALTH).expect("health")
}

/// The health changes told to a client since `from`.
fn health_sent(h: &Harness, b: usize, from: usize) -> Vec<i32> {
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::UpdateInt(q) if q.quality == quality::HEALTH => Some(q.value),
            _ => None,
        })
        .collect()
}

/// The harm of a fall, by its height, as 2017's falls fit: nothing below 3.5 m, then about a point a
/// meter, steepening past 12.5 m to a thousand at 100 m, and no more past the height where a fall
/// reaches its greatest speed; a mounted player takes the height less 2.5 m alone.
/// Behaviour: server.falls.harm-by-height
#[test]
fn a_falls_harm_grows_with_its_height_and_a_mount_takes_the_steep_part() {
    let unmounted: Vec<i32> = [3.0, 3.5, 5.0, 10.0, 20.0, 40.0, 60.0, 100.0, 127.5, 300.0]
        .iter()
        .map(|h| falls::harm(*h, false))
        .collect();
    assert_eq!(unmounted, [0, 1, 2, 7, 18, 72, 217, 1015, 2158, 2158]);
    let mounted: Vec<i32> = [3.0, 10.0, 40.0, 100.0]
        .iter()
        .map(|h| falls::harm(*h, true))
        .collect();
    assert_eq!(mounted, [0, 7, 37, 97]);
}

/// A player whose client reports it dropping 20 m loses 18 health as it lands, told as its health
/// alone; one dropping 3 m loses none.
/// Behaviour: server.falls.a-fall-hurts-as-it-lands
#[test]
fn a_player_falling_twenty_meters_loses_its_harm_as_it_lands() {
    let (mut h, b, world, at) = player(1000);
    let from = h.bots[b].inbox.len();
    let landed = drop(&mut h, b, at.origin, at.cell, 20.0, 0.0);
    assert_eq!(health(&h, world), 982);
    assert_eq!(health_sent(&h, b, from), [982], "told as its health");
    let stories = h
        .world_messages(b, from)
        .iter()
        .filter(|m| matches!(m, ServerMessage::DoStory(_)))
        .count();
    assert_eq!(stories, 0, "with no story");
    drop(&mut h, b, landed, at.cell, 3.0, 0.0);
    assert_eq!(health(&h, world), 982, "3 m is no harm");
}

/// A player running off a ledge falls from the ledge; one riding down a slope as steep as a body
/// rides loses nothing, however far it goes down; and a walk down a hill before a drop is no part of
/// the fall.
/// Behaviour: server.falls.a-fall-hurts-as-it-lands
#[test]
fn a_run_off_a_ledge_is_a_fall_and_a_ride_down_a_slope_is_none() {
    let (mut h, b, world, at) = player(1000);
    let landed = drop(&mut h, b, at.origin, at.cell, 20.0, 6.5);
    assert_eq!(health(&h, world), 982, "off a ledge at a run");
    // Down a slope of 45 degrees at a mount's run: 10.6 m down and across each second, 20 m in all.
    let [x, y, z] = landed;
    for k in 0..=12u16 {
        #[allow(clippy::cast_possible_truncation)]
        let d = ((f64::from(k) * STEP * 10.6) as f32).min(20.0);
        report(&mut h, b, [x + d, y, z - d], at.cell);
    }
    report(&mut h, b, [x + 20.0, y, z - 20.0], at.cell);
    assert_eq!(health(&h, world), 982, "no harm down the slope");
    // A walk down 10 m of a hill, then a drop of 20 m from its foot without a stop.
    let [x, y, z] = [x + 20.0, y, z - 20.0];
    for k in 0..=11u16 {
        #[allow(clippy::cast_possible_truncation)]
        let d = (f64::from(k) * STEP * 4.6) as f32;
        report(&mut h, b, [x + d, y, z - d], at.cell);
    }
    #[allow(clippy::cast_possible_truncation)]
    let foot = (11.0 * STEP * 4.6) as f32;
    // The drop begins at the walk's last report, a step ago.
    let t0 = h.now.0 - STEP;
    let mut fell = 0.0f32;
    while fell < 20.0 {
        #[allow(clippy::cast_possible_truncation)]
        let s = (h.now.0 - t0) as f32;
        fell = (4.9 * s * s).min(20.0);
        report(&mut h, b, [x + foot + 4.6 * s, y, z - foot - fell], at.cell);
    }
    report(&mut h, b, [x + foot + 10.0, y, z - foot - 20.0], at.cell);
    assert_eq!(health(&h, world), 964, "a fall of 20 m, not 30");
}

/// A mounted player's fall from 40 m takes 37, the height less 2.5 m alone.
/// Behaviour: server.falls.harm-by-height
#[test]
fn a_mounted_players_fall_takes_its_height_less_two_and_a_half() {
    let (mut h, b, world, at) = player(1000);
    if let Some(p) = h.server.players.get_mut(&world) {
        p.riding = Some(yalain_world::mounts::Riding {
            eid: 1,
            state: 0,
            look: auberean_protocol::types::VisualDesc::default(),
            speed: 1.0,
            checking: false,
        });
    }
    drop(&mut h, b, at.origin, at.cell, 40.0, 0.0);
    assert_eq!(health(&h, world), 963);
}

/// A player holding Safe Fall's effect lands unhurt from a fall that would harm it, and the effect
/// ends with that fall; a fall that harms nothing leaves it, and the fall after the one it saved
/// hurts.
/// Behaviour: server.falls.safe-fall-and-the-modifier
#[test]
fn safe_fall_spares_one_fall_and_ends() {
    let (mut h, b, world, at) = player(1000);
    h.server.effect_table.effects.insert(
        SAFE,
        EffectDef {
            kind: EffectKind::Numeric {
                flags: 5,
                stats: Vec::new(),
                magnitude: Scale::default(),
            },
            duration: Scale(vec![(1.0, 10.0), (10.0, 30.0)]),
            enum_value: falls::SAFE_FALL,
            internal_flags: yalain_world::effect_defs::CLIENT_VISIBLE,
            ..yalain_testkit::plain_effect(SAFE)
        },
    );
    let cast = yalain_world::effects::Cast {
        spellcraft: 10.0,
        ..yalain_world::effects::Cast::default()
    };
    let clock = h.clock();
    yalain_world::effects::apply(&mut h.server, &mut h.net, world, SAFE, cast, clock);
    let held = |h: &Harness| {
        h.server.players[&world]
            .play
            .effects
            .iter()
            .any(|e| e.state == SAFE)
    };
    assert!(held(&h));
    let landed = drop(&mut h, b, at.origin, at.cell, 3.0, 0.0);
    assert!(held(&h), "a fall that harms nothing leaves it");
    let landed = drop(&mut h, b, landed, at.cell, 20.0, 0.0);
    assert_eq!(health(&h, world), 1000, "spared");
    assert!(!held(&h), "the effect ends with the fall it saved");
    drop(&mut h, b, landed, at.cell, 20.0, 0.0);
    assert_eq!(health(&h, world), 982, "the next fall hurts");
}

/// A player whose fall modifier is 0 takes no harm from a fall; at 0.5, half.
/// Behaviour: server.falls.safe-fall-and-the-modifier
#[test]
fn the_fall_modifier_scales_a_falls_harm() {
    for (modifier, left) in [(0.0, 1000), (0.5, 991)] {
        let (mut h, b, world, at) = player(1000);
        if let Some(p) = h.server.players.get_mut(&world) {
            yalain_world::quals::set_float(&mut p.qualities, falls::FALL_MODIFIER, modifier);
        }
        drop(&mut h, b, at.origin, at.cell, 20.0, 0.0);
        assert_eq!(health(&h, world), left, "modifier {modifier}");
    }
}

/// A fall of 100 m kills a player with 500 health: it dies, by misadventure.
/// Behaviour: server.falls.a-fall-hurts-as-it-lands
#[test]
fn a_fall_that_takes_all_a_players_health_kills_it_by_misadventure() {
    let (mut h, b, world, at) = player(500);
    let from = h.bots[b].inbox.len();
    drop(&mut h, b, at.origin, at.cell, 100.0, 0.0);
    assert_eq!(health(&h, world), 0);
    assert!(h.server.players[&world].dying.is_some(), "it dies");
    let deaths: Vec<(u32, u32)> = h
        .calls(b, from)
        .into_iter()
        .filter(|(f, _)| *f == DEATH_MESSAGE)
        .filter_map(|(_, v)| {
            v.iter().find_map(|x: &Value| match x.root() {
                Some(Node::TextInfo(t)) => Some((t.string_id, t.table)),
                _ => None,
            })
        })
        .collect();
    assert_eq!(
        deaths,
        [yalain_world::death::MISADVENTURE],
        "by misadventure"
    );
}

/// A fall of 40 m into water, landing under its surface, harms nothing; the same fall where the
/// block holds no water takes its harm.
/// Behaviour: server.falls.water-spares
#[test]
fn a_fall_into_water_harms_nothing() {
    const SURFACE: f32 = 70.0;
    for (water, left) in [(Some(SURFACE), 1000), (None, 928)] {
        let (mut h, b, world, _) = player(1000);
        h.server.ground = Some(Box::new(TestGround {
            land: Some(SURFACE - 30.0),
            water,
            ..TestGround::default()
        }));
        let (cell, x, y) = yalain_world::spawn::outdoor(0x0299, 80.0, 80.0);
        if let Some(p) = h.server.players.get_mut(&world) {
            p.position = StoredPosition {
                cell,
                origin: [x, y, SURFACE + 32.0],
                rotation: [1.0, 0.0, 0.0, 0.0],
            };
        }
        drop(&mut h, b, [x, y, SURFACE + 32.0], cell, 40.0, 0.0);
        assert_eq!(health(&h, world), left, "water {water:?}");
    }
}
