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
use auberean_scene::others::{
    is_loaded, out_of_physics_range, position_of, LostCell, LostCells, OtherBody, Received,
};
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
                b.receive(
                    &place(70.0, y, 90.0, 0.0, s, stamp),
                    None,
                    true,
                    &land,
                    &mut w,
                );
                stopped = true;
            } else {
                b.receive(
                    &place(70.0, y, 0.0, 1.0, s, stamp),
                    None,
                    true,
                    &land,
                    &mut w,
                );
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
        b.receive(&far, None, true, &land, &mut w),
        Received::Taken,
        "a newer position is taken"
    );
    let (p, h) = b.drawn(1.0);
    assert!(dist(p, grid(70.0, 40.0)) < 1e-4, "put there: {p:?}");
    assert!((h - 180.0).abs() < 1.0, "facing south: {h}");
    assert!(!b.body.minterp.is_interpolating(), "nothing to follow");
    assert!(
        b.receive(
            &place(70.0, 39.0, 0.0, 0.0, 0.1, 2),
            None,
            true,
            &land,
            &mut w
        ) == Received::Ignored,
        "the same stamp again is ignored"
    );
    // Sent running south, then silent: it runs on past the position, and 2 s on is put back.
    let mut g = OtherBody::new(8, config(), position_of(&start), 180.0, (1, 0));
    let last = place(70.0, 19.0, 180.0, 1.0, 0.1, 2);
    g.receive(&last, None, true, &land, &mut w);
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
/// eight beside it) and the interior cells of those nine blocks are loaded; with no player, no cell
/// is (the client holds none then). A position in a cell
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
    assert!(!loaded(0x4040_0001, None), "with no player, nothing");
    assert!(
        land.release_block(0x41, 0x41).is_some() && !loaded(0x4141_0005, Some(here)),
        "a land cell of a block let go of"
    );
    let start = place(70.0, 20.0, 0.0, 0.0, 0.0, 1);
    let mut b = OtherBody::new(7, config(), position_of(&start), 0.0, (1, 0));
    let mut w = World::new();
    let mut far = place(70.0, 60.0, 90.0, 1.0, 0.3, 2);
    far.cell = 0x4240_0003;
    assert!(matches!(
        b.receive(&far, None, loaded(far.cell, Some(here)), &land, &mut w),
        Received::Lost(_)
    ));
    assert!(!b.in_world(), "out of the world");
    b.re_enter_visibility(&land, &mut w);
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

/// An object lost in a cell goes out of any cell it was lost in first; one that hangs from another
/// is then lost in none. A cell loaded gives back the objects lost in it, in the order they were
/// lost, and none of them is lost any more.
///
/// Behaviour: desktop.others.an-object-hanging-from-another-is-lost-in-no-cell
#[test]
fn an_object_hanging_from_another_is_lost_in_no_cell() {
    let (here, there) = (0x4240_0003, 0x4240_0004);
    let mut lost = LostCells::default();
    assert!(lost.goto(1, here, false) && lost.goto(2, here, false));
    assert_eq!(lost.cell_of(1), here);
    assert!(lost.goto(1, there, false), "moved to another cell");
    assert!(!lost.goto(2, there, true), "it hangs from another");
    assert_eq!(
        (lost.cell_of(2), lost.len()),
        (0, 1),
        "out of the cell it was in"
    );
    assert!(!lost.goto(3, 0, false), "no cell");
    assert!(lost.goto(4, there, false));
    assert_eq!(lost.take(there), vec![1, 4]);
    assert!(lost.is_empty() && lost.take(here).is_empty());
}

/// An object the server made, with no qualities of its own, entity template `entity`.
fn made(entity: u32) -> auberean_client_sdk::model::Object {
    use auberean_client_sdk::protocol::types::{PhysicsDesc, VisualDesc, WeenieDesc};
    auberean_client_sdk::model::Object {
        id: 0x5000_0001,
        instance_stamp: 0,
        visual: VisualDesc::default(),
        visual_updates: 0,
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
        position_stamp: 0,
        creations: 1,
        qualities: auberean_client_sdk::model::Qualities::default(),
        reports: Vec::new(),
    }
}

/// A description the server sends again counts as sent again, though nothing in it changed: retail
/// applies it, so the object's effects play again. A changed one goes the way of a new look, and
/// one already taken is not sent again.
///
/// Behaviour: desktop.looks.a-look-sent-again-plays-its-effects-again
#[test]
fn a_look_sent_again_plays_its_effects_again() {
    use auberean_scene::others::sent_again;
    let mut o = made(1);
    let taken = o.visual.clone();
    assert!(!sent_again(Some(&taken), 0, &o), "nothing sent since");
    o.visual_updates = 1;
    assert!(sent_again(Some(&taken), 0, &o), "the same one again");
    assert!(!sent_again(Some(&taken), 1, &o), "already taken");
    assert!(!sent_again(None, 0, &o), "no look taken yet");
    o.visual.bits ^= 1;
    assert!(!sent_again(Some(&taken), 0, &o), "a changed one");
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

/// A body put in the world's table as its look's shape (one that does not move) is solid at its
/// whole scale, its look's times its own (the scale its qualities give), not at its look's alone:
/// it reaches half again as far when its own scale is 1.5.
///
/// Behaviour: desktop.others.a-still-body-is-solid-at-its-whole-scale
#[test]
fn a_still_body_is_solid_at_its_whole_scale() {
    use auberean_physics::object::Shape;
    use auberean_physics::world::WorldObject;
    use dereth_primitives::shape::CylSphere;
    let land = world();
    let mut table = World::new();
    let look = dereth_primitives::Vec3::new(1.1, 1.1, 1.1);
    let shape = std::sync::Arc::new(Shape {
        trees: Vec::new(),
        cylinders: vec![CylSphere {
            low_pt: dereth_primitives::Vec3::ZERO,
            radius: 0.5,
            height: 2.0,
        }],
        spheres: Vec::new(),
        physics_box: auberean_physics::geom::BBox::default(),
        scale: look,
        containment_radius: 0.5,
        render_box: auberean_physics::geom::BBox::default(),
        properties: Vec::new(),
    });
    let at = position_of(&place(70.0, 20.0, 0.0, 0.0, 0.0, 1));
    let alone = table.insert(
        &land,
        8,
        &WorldObject {
            shape: std::sync::Arc::clone(&shape),
            position: at,
            is_static: true,
            passable: false,
        },
    );
    let mut b = OtherBody::new(7, config(), at, 0.0, (1, 0));
    b.body.look_scale = look;
    b.body.set_scale(1.5);
    let k = b.stand_in_table(&mut table, &land, shape);
    let (whole, by_look) = (
        table.reach(k).expect("in the table"),
        table.reach(alone).expect("in the table"),
    );
    assert!(
        (whole / by_look - 1.5).abs() < 1e-5,
        "{whole} against {by_look} at its look's scale"
    );
}

/// Whether an object moves is asked of one qualities record: its entity template's when its
/// description names an entity (none when the id named is not an entity's), else its script
/// package's (package `0xFFFF` when it names none). The object moves when that record does not say
/// otherwise, never when the data has no such record, and a body that does not move stays where it
/// stands whatever positions are sent for it.
///
/// Behaviour: desktop.others.an-object-moves-as-its-qualities-record-says
#[test]
fn an_object_moves_as_its_qualities_record_says() {
    use auberean_assets::script::Qualities as Record;
    use auberean_client_sdk::protocol::types::WeenieDesc;
    use auberean_scene::profile::{is_mobile, mobility_record, MOBILE};
    let entity = |did: u32| WeenieDesc {
        mask: WeenieDesc::ENTITY | WeenieDesc::PACKAGE,
        entity_did: did,
        package_id: 0x37F,
        ..WeenieDesc::default()
    };
    assert_eq!(mobility_record(&entity(0x4700_0530)), 0x8100_0530);
    assert_eq!(
        mobility_record(&entity(0x0200_0001)),
        0,
        "a setup is no entity"
    );
    let package = WeenieDesc {
        mask: WeenieDesc::PACKAGE,
        entity_did: 0x4700_0530,
        package_id: 0x0001_037F,
        ..WeenieDesc::default()
    };
    assert_eq!(
        mobility_record(&package),
        0x8000_037F,
        "no entity said: the package's"
    );
    assert_eq!(mobility_record(&WeenieDesc::default()), 0x8000_FFFF);

    let says = |v: Option<i32>| {
        Some(Record {
            bools: v.map(|v| vec![(MOBILE, v)]).unwrap_or_default(),
            ..Record::default()
        })
    };
    let w = entity(0x4700_0530);
    assert!(is_mobile(&w, |_| says(Some(1))));
    assert!(is_mobile(&w, |_| says(None)), "a record that does not say");
    assert!(
        !is_mobile(&w, |_| says(Some(0))),
        "a record that says still"
    );
    assert!(!is_mobile(&w, |_| None), "no record");
    let mut asked = false;
    assert!(!is_mobile(&entity(0x0200_0001), |_| {
        asked = true;
        says(Some(1))
    }));
    assert!(!asked, "record 0 is never read");

    // A still body sent a place 2 m off stays; a moving one follows it there.
    let land = world();
    let mut table = World::new();
    let at = place(70.0, 20.0, 0.0, 0.0, 0.0, 1);
    let to = place(72.0, 20.0, 0.0, 0.0, 0.0, 2);
    let mut ends = Vec::new();
    for mobile in [true, false] {
        let mut b = OtherBody::new(7, config(), position_of(&at), 0.0, (1, 0));
        b.body.set_mobile(mobile);
        assert_eq!(
            b.receive(&to, None, true, &land, &mut table),
            Received::Taken
        );
        for i in 1..=30 {
            b.tick(&land, &mut table, &mut NoObjects, 0.1 * f64::from(i));
        }
        ends.push(b.body.position.frame.origin.x);
    }
    assert!((ends[0] - 72.0).abs() < 0.05, "followed: {}", ends[0]);
    assert!((ends[1] - 70.0).abs() < 1e-4, "stayed: {}", ends[1]);
}

/// Behaviour: desktop.others.a-moving-body-past-the-range-stops-until-a-position-comes
#[test]
fn a_moving_body_past_the_range_stops_until_a_position_comes() {
    let at = position_of(&place(70.0, 70.0, 0.0, 0.0, 1.0, 1));
    let mut ob = OtherBody::new(7, config(), at, 0.0, (1, 0));
    ob.body.set_mobile(true);
    let near = position_of(&place(90.0, 70.0, 0.0, 0.0, 1.0, 1));
    let mut far = near;
    far.frame.origin.y += 96.5;
    assert!(!out_of_physics_range(&near, &at));
    assert!(out_of_physics_range(&far, &at));
    // The player's near: it runs on.
    ob.check_range(&near);
    assert!(ob.in_range);
    assert!(ob.body.is_movement_active());
    // The player has walked away and no position has come: it stops where it is, and its sounds
    // are silent.
    ob.check_range(&far);
    assert!(!ob.in_range);
    assert!(!ob.body.is_movement_active());
    assert!(ob.body.should_mute_sound(&|_| None));
    // A body whose movement has stopped doesn't look again; a position does.
    ob.check_range(&near);
    assert!(!ob.in_range);
    let again = place(70.0, 70.0, 0.0, 0.0, 2.0, 2);
    assert!(matches!(
        ob.receive(&again, Some(&near), true, &world(), &mut World::new()),
        Received::Taken
    ));
    assert!(ob.in_range);
    assert!(ob.body.is_movement_active());
    assert!(!ob.body.should_mute_sound(&|_| None));
}

/// Behaviour: desktop.others.a-moving-body-past-the-range-stops-until-a-position-comes
#[test]
fn a_body_that_cannot_move_does_not_look_at_the_range() {
    let at = position_of(&place(70.0, 70.0, 0.0, 0.0, 1.0, 1));
    let mut ob = OtherBody::new(7, config(), at, 0.0, (1, 0));
    ob.body.set_mobile(false);
    let mut far = at;
    far.frame.origin.y += 200.0;
    ob.check_range(&far);
    assert!(ob.in_range);
}

/// A made-up figure: one bone, no animations.
struct FigureRecords(std::sync::Arc<auberean_assets::setup::Setup>);

impl auberean_animation::Animations for FigureRecords {
    fn animation(
        &self,
        _id: auberean_primitives::DataId,
    ) -> Option<std::sync::Arc<auberean_assets::animation::Animation>> {
        None
    }
}

impl auberean_world::look::LookRecords for FigureRecords {
    fn visual(
        &self,
        _id: auberean_primitives::DataId,
    ) -> Option<std::sync::Arc<auberean_assets::visual::VisualDesc>> {
        None
    }
    fn appearance(
        &self,
        _id: auberean_primitives::DataId,
    ) -> Option<std::sync::Arc<auberean_assets::visual::Appearance>> {
        None
    }
}

impl auberean_animation::BodyRecords for FigureRecords {
    fn setup(
        &self,
        _id: auberean_primitives::DataId,
    ) -> Option<std::sync::Arc<auberean_assets::setup::Setup>> {
        Some(self.0.clone())
    }
    fn wind(&self) -> f32 {
        0.0
    }
    fn anim_map(
        &self,
        _id: auberean_primitives::DataId,
    ) -> Option<std::sync::Arc<auberean_assets::animation::AnimMap>> {
        None
    }
    fn behavior_table(
        &self,
        _id: auberean_primitives::DataId,
    ) -> Option<std::sync::Arc<auberean_assets::animation::BehaviorTable>> {
        None
    }
}

fn figure() -> auberean_animation::Body {
    let records = FigureRecords(std::sync::Arc::new(auberean_assets::setup::Setup {
        bones: vec![auberean_assets::setup::Bone {
            parent: u32::MAX,
            id: 1,
        }],
        ..auberean_assets::setup::Setup::default()
    }));
    let mut look = auberean_world::look::Look::default();
    look.add_group(&auberean_world::look::Group {
        key: 1,
        setup: auberean_primitives::DataId(0x0400_0001),
        ..auberean_world::look::Group::default()
    })
    .expect("the group");
    auberean_animation::Body::from_look(&records, &look).expect("the figure")
}

/// A jump another body's animation reaches lifts it, even from rest, with no position sent; its
/// hooks are still handed on. Its figure fires each jump once, so the next step lifts it no more,
/// and a figure that sets off none leaves the body where it stands.
///
/// Behaviour: desktop.others.an-animations-jump-lifts-another-body
#[test]
fn an_animations_jump_lifts_another_body() {
    let land = world();
    let mut w = World::new();
    let at = position_of(&place(70.0, 70.0, 0.0, 0.0, 0.0, 1));
    let mut b = OtherBody::new(7, config(), at, 0.0, (1, 0));
    let mut ticker = Ticker::new(0.0);
    for _ in 0..ticker.ticks(1.0) {
        b.tick(&land, &mut w, &mut NoObjects, 1.0);
    }
    assert!(b.body.contact, "it stands on the ground");
    b.body.set_movement_active(false);

    let mut f = figure();
    assert!(b.take_figure_triggers(&mut f).is_empty());
    assert!(b.body.contact, "nothing set off, it stays");
    assert!(!b.body.is_movement_active());

    f.triggers
        .impulses
        .push(auberean_assets::animation::Impulse {
            jump: true,
            impulse: None,
            number: 0,
            time: 0.5,
        });
    f.triggers.hooks.push(auberean_assets::animation::GameHook {
        kind: 3,
        number: 0,
        data: 0,
        triggered: false,
        time: 0.5,
    });
    assert_eq!(b.take_figure_triggers(&mut f), vec![3]);
    assert!(!b.body.contact, "off the ground");
    assert!(b.body.is_movement_active(), "awake");
    assert!(b.body.velocity.z > 6.0, "up at the jump impulse");
    assert!(f.triggers.impulses.is_empty(), "it fires once");
}

/// A body standing on the ground and put in the air falls: its placement touches only what is
/// there.
///
/// Behaviour: desktop.others.a-body-put-in-the-air-falls
#[test]
fn a_body_put_in_the_air_falls() {
    let land = world();
    let mut w = World::new();
    let start = place(70.0, 20.0, 0.0, 0.0, 0.0, 1);
    let mut b = OtherBody::new(7, config(), position_of(&start), 0.0, (1, 0));
    let mut t = 0.0;
    for _ in 0..5 {
        t += 0.1;
        b.tick(&land, &mut w, &mut NoObjects, t);
    }
    assert!(b.body.contact, "it stands on the ground");
    let mut up = place(70.0, 25.0, 0.0, 0.0, t, 2);
    up.origin.z = GROUND_Z + 3.0;
    up.teleport_stamp = 1;
    let player = position_of(&start);
    assert_eq!(
        b.receive(&up, Some(&player), true, &land, &mut w),
        Received::Taken,
        "a teleport is taken"
    );
    assert!((b.body.position.frame.origin.z - (GROUND_Z + 3.0)).abs() < 1e-3);
    assert!(!b.body.contact, "put in the air, it touches nothing");
    for _ in 0..30 {
        t += 0.1;
        b.tick(&land, &mut w, &mut NoObjects, t);
    }
    assert!(b.body.contact, "it landed");
    assert!((b.body.position.frame.origin.z - GROUND_Z).abs() < 1e-3);
}

/// A body new in the world's table and made in the air falls.
///
/// Behaviour: desktop.others.a-body-put-in-the-air-falls
#[test]
fn a_body_made_in_the_air_falls_once_it_stands_in_the_table() {
    let land = world();
    let mut w = World::new();
    let mut at = position_of(&place(70.0, 20.0, 0.0, 0.0, 0.0, 1));
    at.frame.origin.z = GROUND_Z + 3.0;
    let mut b = OtherBody::new(7, config(), at, 0.0, (1, 0));
    w.insert_body(&land, 7, &mut b.body);
    b.settle(&land, &mut w);
    assert!(!b.body.contact, "it touches nothing");
}

/// A body put where another stands takes a free spot nearby.
///
/// Behaviour: desktop.others.a-body-put-on-another-takes-a-free-spot
#[test]
fn a_body_put_on_another_takes_a_free_spot() {
    let land = world();
    let mut w = World::new();
    let spot = place(90.0, 90.0, 0.0, 0.0, 0.0, 1);
    let mut d = OtherBody::new(9, config(), position_of(&spot), 0.0, (1, 0));
    let mut e = OtherBody::new(
        10,
        config(),
        position_of(&place(110.0, 90.0, 0.0, 0.0, 0.0, 1)),
        0.0,
        (1, 0),
    );
    w.insert_body(&land, 9, &mut d.body);
    w.insert_body(&land, 10, &mut e.body);
    let mut on = place(90.0, 90.0, 0.0, 0.0, 0.1, 2);
    on.teleport_stamp = 1;
    e.receive(&on, None, true, &land, &mut w);
    let (a, b) = (e.body.position.frame.origin, d.body.position.frame.origin);
    let apart = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
    assert!(apart > 0.5, "{apart} m apart");
}

/// A position more than 15 m away, with no teleport, is placed too.
///
/// Behaviour: desktop.others.a-body-put-in-the-air-falls
#[test]
fn a_far_position_is_placed_like_a_teleport() {
    let land = world();
    let mut w = World::new();
    let start = place(70.0, 20.0, 0.0, 0.0, 0.0, 1);
    let mut b = OtherBody::new(7, config(), position_of(&start), 0.0, (1, 0));
    b.tick(&land, &mut w, &mut NoObjects, 0.1);
    let mut far = place(70.0, 60.0, 0.0, 0.0, 0.2, 2);
    far.origin.z = GROUND_Z + 3.0;
    b.receive(&far, None, true, &land, &mut w);
    assert!(!b.body.contact, "put in the air, it touches nothing");
}

/// A body following a position it can't reach is put there when it gives up, and if that is in the
/// air it falls.
///
/// Behaviour: desktop.others.a-body-put-in-the-air-falls
#[test]
fn a_body_put_where_it_could_not_follow_falls() {
    let land = world();
    let mut w = World::new();
    let start = place(70.0, 20.0, 0.0, 0.0, 0.0, 1);
    let mut b = OtherBody::new(7, config(), position_of(&start), 0.0, (1, 0));
    let mut up = place(70.0, 20.0, 0.0, 0.0, 0.1, 2);
    up.origin.z = GROUND_Z + 5.0;
    let player = position_of(&start);
    b.receive(&up, Some(&player), true, &land, &mut w);
    let mut t = 0.1;
    let mut highest = GROUND_Z;
    for _ in 0..200 {
        t += 0.1;
        b.tick(&land, &mut w, &mut NoObjects, t);
        highest = highest.max(b.body.position.frame.origin.z);
    }
    assert!(highest > GROUND_Z + 4.0, "it was put up there: {highest}");
    assert!(b.body.contact, "it landed");
    let z = b.body.position.frame.origin.z;
    assert!((z - GROUND_Z).abs() < 1e-3, "on the ground: {z}");
}

/// A position at (70, 70) facing north that carries a jump, up at the player races' impulse.
fn jump_at(time: f64, stamp: u16) -> Place {
    let mut j = place(70.0, 70.0, 0.0, 0.0, time, stamp);
    j.flags |= auberean_client_sdk::protocol::physics::position_flags::JUMP;
    j.velocity = Vec3::new(0.0, 0.0, 6.26);
    j
}

/// A jump sent for another body that has stood long enough lifts it at once, along the arc a body
/// jumping in place follows: heights over the ground each 0.1 s tick, the same as the final client
/// draws for such a body. It keeps its heading, and the reports of the jumper in the air don't
/// pull it off the arc. It lands where it rose.
///
/// Behaviour: desktop.others.a-jump-sent-for-another-body-lifts-it
#[test]
fn a_jump_sent_for_another_body_lifts_it() {
    const ARC: [f32; 12] = [
        0.577, 1.056, 1.437, 1.720, 1.905, 1.991, 1.980, 1.871, 1.663, 1.358, 0.955, 0.453,
    ];
    let land = world();
    let mut w = World::new();
    let start = place(70.0, 70.0, 0.0, 0.0, 0.0, 1);
    let mut b = OtherBody::new(7, config(), position_of(&start), 0.0, (1, 0));
    // Time on the ground runs only while the body's movement does.
    b.body.time_on_ground = 2.0;
    let mut now = 1.0;
    b.tick(&land, &mut w, &mut NoObjects, now);
    assert!(b.body.contact, "it stands on the ground");

    assert_eq!(
        b.receive(&jump_at(now, 2), None, true, &land, &mut w),
        Received::Taken
    );
    let mut stamp = 3;
    for (i, want) in ARC.iter().enumerate() {
        now += 0.1;
        b.tick(&land, &mut w, &mut NoObjects, now);
        let up = b.body.position.frame.origin.z - GROUND_Z;
        assert!((up - want).abs() < 0.005, "tick {i}: {up}, not {want}");
        assert!(!b.body.contact, "tick {i}: off the ground");
        assert!(
            b.body.heading.abs() < 0.5,
            "tick {i}: turned to {}",
            b.body.heading
        );
        if i % 2 == 1 {
            let mut air = place(70.0, 70.0, 0.0, 0.0, now, stamp);
            air.origin.z = GROUND_Z + want;
            b.receive(&air, None, true, &land, &mut w);
            stamp += 1;
        }
    }
    now += 0.1;
    b.tick(&land, &mut w, &mut NoObjects, now);
    assert!(b.body.contact, "down again");
    assert!((b.body.position.frame.origin.z - GROUND_Z).abs() < 0.001);
    assert!((b.body.position.frame.origin.x - 70.0).abs() < 0.01);
    assert!((b.body.position.frame.origin.y - 70.0).abs() < 0.01);
}

/// A jump sent for a body with under half a second on the ground, counting only the ticks its
/// movement ran, does not lift it. It stays on the ground, as it does in the final client. Those
/// ticks then count, so the next jump sent lifts it.
///
/// Behaviour: desktop.others.a-jump-too-soon-does-not-lift-another-body
#[test]
fn a_jump_too_soon_does_not_lift_another_body() {
    let land = world();
    let mut w = World::new();
    let start = place(70.0, 70.0, 0.0, 0.0, 0.0, 1);
    let mut b = OtherBody::new(7, config(), position_of(&start), 0.0, (1, 0));
    let mut now = 1.0;
    b.tick(&land, &mut w, &mut NoObjects, now);

    b.receive(&jump_at(now, 2), None, true, &land, &mut w);
    let mut stamp = 3;
    for i in 0..20 {
        now += 0.1;
        b.tick(&land, &mut w, &mut NoObjects, now);
        assert!(b.body.contact, "tick {i}: on the ground");
        assert!((b.body.position.frame.origin.z - GROUND_Z).abs() < 0.001);
        if i % 2 == 1 && i < 14 {
            // The jumper rises and falls in its reports; this body only turns to follow.
            let mut air = place(70.0, 70.0, 0.0, 0.0, now, stamp);
            air.origin.z = GROUND_Z + 1.5;
            b.receive(&air, None, true, &land, &mut w);
            stamp += 1;
        }
    }
    assert!(
        b.body.time_on_ground >= 0.5,
        "following counted as standing"
    );

    b.receive(&jump_at(now, stamp), None, true, &land, &mut w);
    for _ in 0..5 {
        now += 0.1;
        b.tick(&land, &mut w, &mut NoObjects, now);
    }
    assert!(!b.body.contact, "the next one lifts it");
    assert!(b.body.position.frame.origin.z - GROUND_Z > 1.5);
}
