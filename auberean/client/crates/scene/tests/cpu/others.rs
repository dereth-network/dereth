//! Another body steered by the positions the server sends for it.
//! Fixture: a flat 160 m block at height 10; a body of the player races' ground values (run
//! 6.5 m/s, turns 120 degrees a second, accelerations 15); positions every 0.25 s, the cadence the
//! recorded sessions' servers sent while a body moved (every two or three of their 0.1 s ticks),
//! stamped with the server's clock, which the game clock here matches; frames every 1/60 s.

use auberean_assets::animation::MotionParams;
use auberean_client_sdk::model::model::Place;
use auberean_physics::body::missile::NoObjects;
use auberean_physics::body::{BodyConfig, Ticker};
use auberean_physics::land::{Block, Land};
use auberean_physics::motion::values::default_values;
use auberean_physics::world::World;
use auberean_primitives::Vec3;
use auberean_scene::others::{is_loaded, position_of, LostCell, OtherBody, Received};
use dereth_physics::geom::Sphere;

const FRAME: f64 = 1.0 / 60.0;
const EVERY: f64 = 0.25;
const GROUND_Z: f32 = 10.0;
/// The block the fixture stands in, and its first cell.
const BLOCK: u32 = 0x4040_0000;

fn ground() -> MotionParams {
    MotionParams {
        jump_impulse: 6.26,
        max_forward: 6.5,
        max_backward: 4.0,
        max_strafe: 4.5,
        still_turn_rate: 90.0,
        running_turn_rate: 120.0,
        forward_slowdown: 18.0,
        backward_slowdown: 25.0,
        strafe_slowdown: 25.0,
        turn_slowdown: 1850.0,
        forward_acceleration: 15.0,
        backward_acceleration: 15.0,
        strafe_acceleration: 15.0,
        turn_acceleration: 1000.0,
        ..default_values()
    }
}

fn world() -> Land {
    let land = Land::new();
    land.insert(Block::flat(0x40, 0x40, GROUND_Z));
    land
}

fn config() -> BodyConfig {
    BodyConfig {
        spheres: vec![Sphere::new(
            dereth_primitives::Vec3::new(0.0, 0.0, 0.48),
            0.48,
        )],
        step_up: 0.6,
        step_down: 0.6,
        scale: 1.0,
        ground: ground(),
        air: MotionParams {
            gravity: 1,
            ..default_values()
        },
    }
}

/// A position: at (x, y) of the block on the ground, facing `heading` (degrees), asking for
/// `forward` (-1 to 1, as the byte carries it), touching the ground, sent at `time`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn place(x: f32, y: f32, heading: f32, forward: f32, time: f64, stamp: u16) -> Place {
    let cell = BLOCK | ((x / 20.0) as u32 * 8 + (y / 20.0) as u32 + 1);
    let h = ((heading / 360.0) * 255.0).round() as u32 & 0xFF;
    let f = ((forward.clamp(-1.0, 1.0) + 1.0) * 127.5) as u32;
    Place {
        cell,
        origin: Vec3::new(x, y, GROUND_Z),
        heading_motion: h << 24 | 127 << 16 | f.min(255) << 8 | 127,
        position_stamp: stamp,
        teleport_stamp: 0,
        force_position_stamp: 0,
        time,
        flags: auberean_client_sdk::protocol::physics::position_flags::CONTACT,
        velocity: Vec3::new(0.0, 0.0, 0.0),
    }
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// On the one grid across every block.
fn grid(x: f32, y: f32) -> Vec3 {
    Vec3::new(0x40 as f32 * 160.0 + x, 0x40 as f32 * 160.0 + y, GROUND_Z)
}

/// A body running north at 6.5 m/s from (70, 20) sent every quarter second until `stop_at`,
/// then once standing where it stopped facing east; the drawn places of each frame up to `until`,
/// and the body.
fn run(stop_at: f64, until: f64) -> (Vec<(f64, Vec3, f32)>, OtherBody) {
    let land = world();
    let mut w = World::new();
    let start = place(70.0, 20.0, 0.0, 0.0, 0.0, 1);
    let mut b = OtherBody::new(7, config(), position_of(&start), 0.0, (1, 0));
    let mut ticker = Ticker::new(0.0);
    let mut drawn = Vec::new();
    let mut next = EVERY;
    let mut stamp = 1;
    let mut stopped = false;
    let mut t = 0.0;
    while t < until {
        t += FRAME;
        if t >= next && !stopped {
            let s = next.min(stop_at);
            #[allow(clippy::cast_possible_truncation)]
            let y = 20.0 + 6.5 * s as f32;
            stamp += 1;
            if next >= stop_at {
                b.receive(&place(70.0, y, 90.0, 0.0, s, stamp), None, true);
                stopped = true;
            } else {
                b.receive(&place(70.0, y, 0.0, 1.0, s, stamp), None, true);
            }
            next += EVERY;
        }
        for _ in 0..ticker.ticks(t) {
            b.tick(&land, &mut w, &mut NoObjects, t);
        }
        let (p, h) = b.drawn(ticker.alpha(t));
        drawn.push((t, p, h));
    }
    (drawn, b)
}

/// Sent every quarter second while it runs, another body is drawn moving at an even pace (from
/// frame to frame within a tenth of its 6.5 m/s once under way), never more than 4 m (about half
/// a second of its run) behind where it was last sent, never ahead of where it truly is, facing
/// north; it does not lurch toward each position and stall before the next.
///
/// Behaviour: desktop.others.another-body-follows-its-reports-at-an-even-pace
#[test]
fn another_body_follows_its_reports_at_an_even_pace() {
    let (drawn, _) = run(10.0, 4.0);
    let mut speeds = Vec::new();
    for w in drawn.windows(2) {
        let (t0, p0, _) = w[0];
        let (t1, p1, _) = w[1];
        #[allow(clippy::cast_possible_truncation)]
        let v = dist(p1, p0) / (t1 - t0) as f32;
        if t0 > 1.5 {
            speeds.push(v);
        }
    }
    let lo = speeds.iter().copied().fold(f32::MAX, f32::min);
    let hi = speeds.iter().copied().fold(0.0f32, f32::max);
    assert!(
        lo > 6.5 * 0.9 && hi < 6.5 * 1.1,
        "an even pace: {lo} to {hi} m/s"
    );
    for (t, p, h) in &drawn {
        if *t > 1.0 {
            #[allow(clippy::cast_possible_truncation)]
            let sent = grid(70.0, 20.0 + 6.5 * ((t / EVERY).floor() * EVERY) as f32).y;
            #[allow(clippy::cast_possible_truncation)]
            let truly = grid(70.0, 20.0 + 6.5 * *t as f32).y;
            assert!(
                p.y > sent - 4.0 && p.y <= truly && (p.x - grid(70.0, 0.0).x).abs() < 1e-2,
                "near its positions at {t}: {p:?}, last sent y {sent}, truly {truly}"
            );
            assert!(
                h.abs() < 0.5 || (h - 360.0).abs() < 0.5,
                "facing north: {h}"
            );
        }
    }
}

/// A body whose last position stands still comes to rest on it, turned to the heading sent.
///
/// Behaviour: desktop.others.another-body-stops-on-its-last-report
#[test]
fn another_body_stops_on_its_last_report() {
    let (drawn, b) = run(2.0, 5.0);
    let (_, p, h) = drawn[drawn.len() - 1];
    let there = grid(70.0, 33.0);
    assert!(dist(p, there) < 0.05, "on its last position: {p:?}");
    assert!((h - 90.0).abs() < 1.5, "facing east: {h}");
    let v = b.body.velocity;
    assert!(v.x.abs() < 1e-3 && v.y.abs() < 1e-3, "at rest: {v:?}");
    assert!(!b.body.minterp.is_interpolating(), "nothing left to follow");
}

/// A position more than 15 m from where a body is puts it there at once; a body that passes no
/// position for 2 s is put on its last one; a position no newer than the last taken is ignored.
///
/// Behaviour: desktop.others.far-reports-and-stuck-bodies-are-put-in-place
#[test]
fn far_reports_and_stuck_bodies_are_put_in_place() {
    let land = world();
    let mut w = World::new();
    let start = place(70.0, 20.0, 0.0, 0.0, 0.0, 1);
    let mut b = OtherBody::new(7, config(), position_of(&start), 0.0, (1, 0));
    let far = place(70.0, 40.0, 180.0, 1.0, 0.05, 2);
    assert_eq!(
        b.receive(&far, None, true),
        Received::Taken,
        "a newer position is taken"
    );
    let (p, h) = b.drawn(1.0);
    assert!(dist(p, grid(70.0, 40.0)) < 1e-4, "put there: {p:?}");
    assert!((h - 180.0).abs() < 1.0, "facing south: {h}");
    assert!(!b.body.minterp.is_interpolating(), "nothing to follow");
    assert!(
        b.receive(&place(70.0, 39.0, 0.0, 0.0, 0.1, 2), None, true) == Received::Ignored,
        "the same stamp again is ignored"
    );
    // Sent running south, then silent: it runs on past the position, and 2 s on is put back.
    let mut g = OtherBody::new(8, config(), position_of(&start), 180.0, (1, 0));
    let last = place(70.0, 19.0, 180.0, 1.0, 0.1, 2);
    g.receive(&last, None, true);
    let mut ticker = Ticker::new(0.1);
    let mut t = 0.1;
    let mut farthest = 0.0f32;
    let mut put_back = false;
    while t < 3.0 {
        t += FRAME;
        for _ in 0..ticker.ticks(t) {
            let before = dist(g.drawn(1.0).0, grid(70.0, 19.0));
            g.tick(&land, &mut w, &mut NoObjects, t);
            let after = dist(g.drawn(1.0).0, grid(70.0, 19.0));
            farthest = farthest.max(before);
            // Put back on (or a tick's run on from) its last position.
            put_back |= before > 5.0 && after < 1.0;
        }
    }
    assert!(farthest > 5.0, "it ran on past the position: {farthest} m");
    assert!(put_back, "put back on its last position");
}

/// Only the land cells of the landblocks the land holds (the near window: the player's and the
/// eight beside it) and the interior cells of those nine blocks are loaded. A position in a cell
/// not loaded takes the body out of the world (it is neither moved nor drawn) and keeps it where
/// it was sent; when it enters the world again it is there. An object taken out of a cell's lost
/// list leaves the last one in its place.
///
/// Behaviour: desktop.others.objects-in-cells-not-loaded-wait-out-of-the-world
#[test]
fn objects_in_cells_not_loaded_wait_out_of_the_world() {
    let here = BLOCK | 1;
    let land = Land::new();
    for x in 0x3Fu8..=0x41 {
        for y in 0x3Fu8..=0x41 {
            land.insert(Block::flat(x, y, 0.0));
        }
    }
    let loaded = |c: u32, at: Option<u32>| is_loaded(c, at, &land);
    assert!(loaded(0x4141_0005, Some(here)) && loaded(0x3F3F_0100, Some(here)));
    assert!(!loaded(0x4240_0001, Some(here)) && !loaded(0x403E_0001, Some(here)));
    assert!(loaded(0x0101_0001, None), "with no player, everything");
    assert!(
        land.release_block(0x41, 0x41).is_some() && !loaded(0x4141_0005, Some(here)),
        "a land cell of a block let go of"
    );
    let start = place(70.0, 20.0, 0.0, 0.0, 0.0, 1);
    let mut b = OtherBody::new(7, config(), position_of(&start), 0.0, (1, 0));
    let mut far = place(70.0, 60.0, 90.0, 1.0, 0.3, 2);
    far.cell = 0x4240_0003;
    assert!(matches!(
        b.receive(&far, None, loaded(far.cell, Some(here))),
        Received::Lost(_)
    ));
    assert!(!b.in_world(), "out of the world");
    b.re_enter_visibility();
    assert!(b.in_world());
    assert_eq!(b.body.position.cell.0, 0x4240_0003, "where it was sent");
    let (_, h) = b.drawn(0.5);
    assert!((h - 90.0).abs() < 1.0, "facing as sent: {h}");
    let mut lost = LostCell::new(0x4240_0003);
    for id in [1, 2, 3, 4] {
        lost.add(id);
    }
    assert!(lost.remove(2) && !lost.remove(9));
    assert_eq!(lost.objects, vec![1, 4, 3]);
}

/// An object the server made, with no qualities of its own, entity template `entity`.
fn made(entity: u32) -> auberean_client_sdk::model::Object {
    use auberean_client_sdk::protocol::types::{PhysicsDesc, VisualDesc, WeenieDesc};
    auberean_client_sdk::model::Object {
        id: 0x5000_0001,
        instance_stamp: 0,
        visual: VisualDesc::default(),
        physics: PhysicsDesc::default(),
        weenie: WeenieDesc {
            entity_did: entity,
            ..WeenieDesc::default()
        },
        place: None,
        parent: None,
        parent_placement: 0,
        container: None,
        wielder: None,
        in_world: true,
        teleport_stamp: None,
        creations: 1,
        qualities: auberean_client_sdk::model::Qualities::default(),
        reports: Vec::new(),
    }
}

/// An object's body takes the profile its qualities give, asked as the object answers them: its
/// description (the scale it holds), then its own qualities, then its qualities record (the one
/// its qualities name, else its entity template's). The player template's record says its body
/// slides along edges and does not ignore collisions; the object's own quality saying it does not
/// slide wins over the record; an object with no record ignores collisions and does not slide.
///
/// Behaviour: desktop.others.a-body-takes-the-profile-its-objects-qualities-give
#[test]
fn a_body_takes_the_profile_its_objects_qualities_give() {
    use auberean_assets::script::Qualities as Record;
    use auberean_physics::body::flags;
    use auberean_scene::profile::profile_of;
    let player_record = || Record {
        bools: vec![(1, 0), (2, 1), (7, 1), (8, 1), (9, 1)],
        ..Record::default()
    };
    let mut asked = Vec::new();
    let mut o = made(0x4700_0530);
    let p = profile_of(&o, |id| {
        asked.push(id);
        Some(player_record())
    });
    assert_eq!(asked, vec![0x8100_0530], "the entity template's record");
    assert_eq!(
        p.state,
        flags::REPORT_COLLISIONS | flags::GRAVITY | flags::EDGE_SLIDE
    );

    o.qualities.base = 0x8000_037F;
    o.qualities.bools.insert(7, 0);
    o.weenie.mask |= auberean_client_sdk::protocol::types::WeenieDesc::SCALE;
    o.weenie.scale = 1.25;
    let p = profile_of(&o, |id| {
        assert_eq!(id, 0x8000_037F, "the record its qualities name");
        Some(Record {
            floats: vec![(0x12A, 2.0)],
            ..player_record()
        })
    });
    assert_eq!(p.state & flags::EDGE_SLIDE, 0, "its own quality first");
    assert_eq!(p.scale, 1.25, "its description's scale first");

    let mut b = OtherBody::new(
        7,
        config(),
        position_of(&place(70.0, 20.0, 0.0, 0.0, 0.0, 1)),
        0.0,
        (1, 0),
    );
    b.body.apply_profile(&profile_of(&made(0x1234), |_| None));
    assert!(!b.body.does_edge_slide() && b.body.ignores_collisions());
}
