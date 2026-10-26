//! Behaviour: none (tooling: the world editor's flying camera over the retail dats)
//! With Collision on, the flying camera slides along what it flies into: a wall met at 45
//! degrees, along it into the corner, where it stops; the floor met at a shallow angle, along it
//! up to the wall; sloping ground it is lowered onto, down it till the ground levels out. Flown
//! along the ground into a stair it steps up each step and flies on at the height of its top; a
//! block too tall stops it, and flown clear of the ground it steps up nothing. Positions and
//! frames are counted, never time.
//! Fixture: Holtburg Redoubt's room `0x0150`, ten meters square and six high, walled east, west
//! and north; Holtburg's level ground south of the house at (79.5, 37.5), with retail slabs two
//! meters square and 0.3 m high placed on it; the slope north of the town.

use dereth_primitives::Vec3;
use lyceum_editor::app::{EditorApp, FLY_RADIUS};
use lyceum_foundry::pick;
use lyceum_foundry::script::Action as Edit;
use lyceum_foundry::source::{Height, Place, Rotation};

use crate::world::dungeon::{put_camera, REDOUBT};
use crate::{editor, fly_for, gpu_lock, sample, HOLTBURG};

/// A room of Holtburg Redoubt walled east, west and north, open to the south.
const ROOM: u16 = 0x0150;

/// The room's lowest and highest corners, in its landblock's meters.
fn room_box(ed: &EditorApp) -> (Vec3, Vec3) {
    let c = ed.session.cells(REDOUBT).remove(&ROOM).unwrap();
    let e = ed.session.environment(c.environment).unwrap();
    let vs = &e.cells[usize::from(c.cell_struct)].vertex_array.vertices;
    let mut lo = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    let mut hi = Vec3::new(f32::MIN, f32::MIN, f32::MIN);
    for v in vs {
        let p = dereth_primitives::frame::localtoglobal(&c.frame, v.position);
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    (lo, hi)
}

/// In Holtburg Redoubt with Collision on, the camera put `from` meters from the room's
/// south-west floor corner, turned `yaw` and pitched `pitch` (degrees), flying at 10 m/s; the
/// room's corners.
fn in_room(
    tag: &str,
    from: Vec3,
    yaw: f32,
    pitch: f32,
) -> (dereth_dat::testing::ScratchDir, EditorApp, Vec3, Vec3) {
    let (dir, world) = sample(tag);
    let mut ed = editor(&world);
    ed.enter_dungeon(REDOUBT).unwrap();
    ed.frame();
    let (lo, hi) = room_box(&ed);
    let at = Vec3::new(lo.x + from.x, lo.y + from.y, lo.z + from.z);
    put_camera(&mut ed, REDOUBT, at, yaw, pitch);
    ed.set_speed(3); // 10 m/s
    ed.frame();
    ed.collide = true;
    (dir, ed, lo, hi)
}

/// Hold W for `frames` frames: where the camera was before and after each, in the room's
/// landblock's meters.
fn fly(ed: &mut EditorApp, frames: u32) -> Vec<Vec3> {
    let o = lyceum_foundry::pick::block_origin(REDOUBT);
    fly_for(ed, 0x57, frames)
        .into_iter()
        .map(|p| Vec3::new(p.x - o.x, p.y - o.y, p.z - o.z))
        .collect()
}

fn distance(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z).magnitude()
}

#[test]
fn with_collide_on_the_flying_camera_flown_at_a_wall_at_45_degrees_slides_along_it_into_the_corner_and_stops_there(
) {
    let _gpu = gpu_lock();
    // Two meters north of the room's south side, halfway across, flying north-east: it meets
    // the east wall short of the north one.
    let (_dir, mut ed, _, hi) = in_room("fly-wall", Vec3::new(5.0, 2.0, 1.5), -45.0, 0.0);
    let path = fly(&mut ed, 60);
    // A frame's flight, before it meets anything: north and east alike.
    let step = distance(path[1], path[2]);
    assert!(step > 0.1, "it flies: {step}");
    let half = step * std::f32::consts::FRAC_1_SQRT_2;
    assert!((path[2].x - path[1].x - half).abs() < 0.01, "{path:?}");
    let (east, north) = (hi.x - FLY_RADIUS, hi.y - FLY_RADIUS);
    for p in &path {
        assert!(
            p.x < east + 0.01 && p.y < north + 0.01,
            "never into a wall: {p:?}"
        );
    }
    // Against the east wall, a frame's flight short of the north one, each frame takes it north
    // by the part of its flight along the wall, and no way east.
    let along: Vec<&[Vec3]> = path
        .windows(2)
        .filter(|w| w[0].x > east - 0.05 && w[1].y < north - step)
        .collect();
    assert!(
        along.len() >= 8,
        "slid along the wall {} frames",
        along.len()
    );
    for w in &along {
        let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
        assert!(
            (dy - half).abs() < 0.02 * step && dx.abs() < 0.01,
            "along the wall by {dy} north and {dx} east, from {:?}",
            w[0]
        );
    }
    // In the corner it stops.
    let end = path[path.len() - 1];
    assert!(
        (end.x - east).abs() < 0.05 && (end.y - north).abs() < 0.05,
        "in the corner at ({east}, {north}): {end:?}"
    );
    assert!(
        distance(path[path.len() - 11], end) < 1e-3,
        "and stopped there: {:?} then {end:?}",
        path[path.len() - 11]
    );
    assert!(ed.collider.as_ref().unwrap().stopped > 0);
}

#[test]
fn with_collide_on_the_flying_camera_flown_at_the_floor_at_a_shallow_angle_slides_along_it_up_to_the_wall(
) {
    let _gpu = gpu_lock();
    // A meter north of the room's south side and a meter over its floor, flying north and down
    // at 15 degrees: it meets the floor a few meters on and has most of the room to slide.
    let (_dir, mut ed, lo, hi) = in_room("fly-floor", Vec3::new(5.0, 1.0, 1.0), 0.0, -15.0);
    let path = fly(&mut ed, 60);
    let step = distance(path[1], path[2]);
    assert!(step > 0.1, "it flies: {step}");
    let ahead = step * dereth_primitives::num::math::cosf(15.0_f32.to_radians());
    let (floor, north) = (lo.z + FLY_RADIUS, hi.y - FLY_RADIUS);
    for p in &path {
        assert!(p.z > floor - 0.01, "never under the floor: {p:?}");
    }
    // On the floor, a frame's flight short of the wall, each frame takes it north by the part
    // of its flight along the floor, and no way down.
    let along: Vec<&[Vec3]> = path
        .windows(2)
        .filter(|w| w[0].z < floor + 0.05 && w[1].y < north - step)
        .collect();
    assert!(
        along.len() >= 10,
        "slid along the floor {} frames",
        along.len()
    );
    for w in &along {
        let (dy, dz) = (w[1].y - w[0].y, w[1].z - w[0].z);
        assert!(
            (dy - ahead).abs() < 0.02 * step && dz.abs() < 0.01,
            "along the floor by {dy} north and {dz} up, from {:?}",
            w[0]
        );
    }
    // At the wall, where it meets the floor, it stops.
    let end = path[path.len() - 1];
    assert!(
        (end.y - north).abs() < 0.05 && (end.z - floor).abs() < 0.05,
        "at the foot of the wall ({north}, {floor}): {end:?}"
    );
    assert!(
        distance(path[path.len() - 11], end) < 1e-3,
        "and stopped there: {:?} then {end:?}",
        path[path.len() - 11]
    );
}

#[test]
fn with_collide_on_the_flying_camera_lowered_onto_sloping_ground_slides_down_it_on_the_ground_till_it_levels_out(
) {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-collide-slope");
    let mut ed = editor(&world);
    let o = pick::block_origin(HOLTBURG);
    let ground =
        |ed: &EditorApp, p: Vec3| ed.session.ground(HOLTBURG, p.x - o.x, p.y - o.y).unwrap();
    // Three meters over the slope north of the town, which falls away north to level ground.
    let g = ed.session.ground(HOLTBURG, 120.0, 110.0).unwrap();
    let north = ed.session.ground(HOLTBURG, 120.0, 125.0).unwrap();
    assert!(
        north < g - 3.0,
        "the ground falls away north: {g} to {north}"
    );
    ed.camera.position = Vec3::new(o.x + 120.0, o.y + 110.0, g + 3.0);
    ed.camera.yaw = 0.0;
    ed.camera.pitch = 0.0;
    ed.set_speed(4); // 25 m/s
    ed.collide = true;
    ed.frame();
    // Shift lowers it straight down, and the slope takes what is along it.
    let trail = fly_for(&mut ed, 0x10, 90);
    let r = lyceum_editor::app::FLY_RADIUS;
    let mut on = 0;
    for p in &trail {
        let under = ground(&ed, *p);
        assert!(
            p.z > under + r - 0.05,
            "never under the ground: {p:?} over {under}"
        );
        on += usize::from(p.z < under + r + 0.35);
    }
    let end = *trail.last().unwrap();
    assert!(
        end.y - o.y > 110.0 + 5.0,
        "slid down the slope, north: {end:?}"
    );
    assert!(on > 30, "on the ground for {on} frames");
    let n = trail.len();
    assert!(
        Vec3::new(
            end.x - trail[n - 11].x,
            end.y - trail[n - 11].y,
            end.z - trail[n - 11].z
        )
        .magnitude()
            < 1e-3,
        "at rest where the ground levels out: {:?} then {end:?}",
        trail[n - 11]
    );
    assert!(
        end.z < ground(&ed, end) + r + 0.35,
        "on the ground: {end:?}"
    );
}

/// A retail slab: two meters square, its top flat 0.3 m over its base.
const SLAB: u32 = 0x0100_3337;

/// How high the slab is.
const SLAB_HIGH: f32 = 0.3;

/// Where the steps are tried: Holtburg's level ground south of the house at (79.5, 37.5).
const X: f32 = 79.5;

/// The ground at `(X, y)`.
fn ground_at(ed: &EditorApp, y: f32) -> f32 {
    ed.session.ground(HOLTBURG, X, y).unwrap()
}

/// A slab placed with its middle at `(X, y)` and its base `base` meters over the ground there.
fn slab(ed: &mut EditorApp, y: f32, base: f32) {
    let z = ground_at(ed, y) + base;
    ed.edit(&Edit::Op(lyceum_foundry::edit::Op::PlaceStatic {
        landblock: HOLTBURG,
        model: dereth_primitives::DataId(SLAB),
        at: Place {
            x: X,
            y,
            z: Height::At(z),
        },
        rotation: Rotation::Heading(0.0),
        name: None,
    }))
    .unwrap();
}

/// With Collision on, the camera at `(X, y)`, its sphere's underside `over` meters over the
/// ground, looking north and flying at 10 m/s.
fn over_the_ground(ed: &mut EditorApp, y: f32, over: f32) {
    let o = pick::block_origin(HOLTBURG);
    let g = ground_at(ed, y);
    ed.collide = false;
    ed.camera.position = Vec3::new(o.x + X, o.y + y, g + FLY_RADIUS + over);
    ed.camera.yaw = 0.0;
    ed.camera.pitch = 0.0;
    ed.set_speed(3); // 10 m/s
    ed.frame();
    ed.collide = true;
}

/// Hold W for `frames` frames: where the camera was before and after each, in Holtburg's meters.
fn fly_north(ed: &mut EditorApp, frames: u32) -> Vec<Vec3> {
    let o = pick::block_origin(HOLTBURG);
    fly_for(ed, 0x57, frames)
        .into_iter()
        .map(|p| Vec3::new(p.x - o.x, p.y - o.y, p.z))
        .collect()
}

/// A block of `n` slabs stacked, with its middle at `(X, y)`.
fn block(ed: &mut EditorApp, y: f32, n: u8) {
    for i in 0..n {
        slab(ed, y, f32::from(i) * SLAB_HIGH);
    }
}

/// The world with a stair of two slabs: the lower on the ground over y 21 to 23, the upper on
/// it over y 22 to 24, its top 0.6 m over the ground; and the ground's height there.
fn by_a_stair(tag: &str) -> (dereth_dat::testing::ScratchDir, EditorApp, f32) {
    let (dir, world) = sample(tag);
    let mut ed = editor(&world);
    slab(&mut ed, 22.0, 0.0);
    slab(&mut ed, 23.0, SLAB_HIGH);
    let g = ground_at(&ed, 22.0);
    assert!((ground_at(&ed, 16.0) - g).abs() < 1e-3, "level ground");
    (dir, ed, g)
}

#[test]
fn with_collide_on_the_flying_camera_flown_along_the_ground_into_a_stair_steps_up_each_step_and_flies_on_at_the_height_of_its_top(
) {
    let _gpu = gpu_lock();
    let (_dir, mut ed, g) = by_a_stair("fly-stair");
    // Its underside a centimeter over the ground, five meters south of the stair.
    over_the_ground(&mut ed, 16.0, 0.01);
    let path = fly_north(&mut ed, 36);
    for (i, p) in path.iter().enumerate() {
        println!("stair {i}: {:.3} {:.3} {:.3}", p.x, p.y, p.z - g);
    }
    let step = path[2].y - path[1].y;
    assert!(step > 0.3, "it flies: {step}");
    // Every frame took it a frame's flight north, the ones it stepped in too, and no way aside.
    for w in path.windows(2) {
        assert!(
            (w[1].y - w[0].y - step).abs() < 1e-3 && (w[1].x - w[0].x).abs() < 1e-3,
            "a frame's flight north from {:?} to {:?}",
            w[0],
            w[1]
        );
    }
    // Up the first step where it met the lower slab's south side, up the second where it met
    // the upper's, and never down.
    let low = g + FLY_RADIUS + 0.01;
    let first = g + SLAB_HIGH + FLY_RADIUS;
    let top = g + 2.0 * SLAB_HIGH + FLY_RADIUS;
    for p in &path {
        let meets = |side: f32| p.y > side - FLY_RADIUS - step && p.y < side - FLY_RADIUS + step;
        if meets(21.0) || meets(22.0) {
            continue;
        }
        let want = if p.y < 21.0 {
            low
        } else if p.y < 22.0 {
            first
        } else {
            top
        };
        assert!(
            (p.z - want).abs() < 1e-3,
            "at {p:?} it is {} over the ground",
            p.z - g
        );
    }
    for w in path.windows(2) {
        assert!(w[1].z >= w[0].z - 1e-4, "never down: {:?} {:?}", w[0], w[1]);
    }
    let end = path[path.len() - 1];
    assert!(end.y > 27.0, "on north past the stair: {end:?}");
    assert!((end.z - top).abs() < 1e-3, "at the top's height: {end:?}");
}

#[test]
fn with_collide_on_the_flying_camera_flown_along_the_ground_into_a_block_too_tall_to_step_onto_stops_at_it_at_its_own_height(
) {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("fly-block");
    let mut ed = editor(&world);
    // Four slabs stacked, 1.2 m: more than a walking body's step up lifts it, however many lifts
    // the slabs' tops give it.
    block(&mut ed, 22.0, 4);
    let g = ground_at(&ed, 22.0);
    over_the_ground(&mut ed, 16.0, 0.01);
    let path = fly_north(&mut ed, 36);
    let low = g + FLY_RADIUS + 0.01;
    for p in &path {
        assert!((p.z - low).abs() < 1e-3, "at its own height: {p:?}");
        assert!(
            p.y < 21.0 - FLY_RADIUS + 1e-3,
            "never into the block: {p:?}"
        );
    }
    let end = path[path.len() - 1];
    assert!(
        end.y > 21.0 - FLY_RADIUS - 0.4,
        "stopped at the block: {end:?}"
    );
    assert!(
        (path[path.len() - 11].y - end.y).abs() < 1e-3,
        "and stays there: {:?} then {end:?}",
        path[path.len() - 11]
    );
}

#[test]
fn with_collide_on_the_flying_camera_flown_clear_of_the_ground_over_the_stair_or_into_a_tall_block_keeps_its_height(
) {
    let _gpu = gpu_lock();
    let (_dir, mut ed, g) = by_a_stair("fly-clear");
    // Its underside 0.7 m over the ground: over the stair's top, which it never touches.
    over_the_ground(&mut ed, 16.0, 0.7);
    let high = g + FLY_RADIUS + 0.7;
    let over = fly_north(&mut ed, 36);
    assert!(over[over.len() - 1].y > 27.0, "{:?}", over[over.len() - 1]);
    for p in &over {
        assert!((p.z - high).abs() < 1e-4, "its height unchanged: {p:?}");
    }
    // Half a meter over the ground, more than its sphere's radius, into a block of three slabs,
    // 0.9 m, whose top is just over its center: it stops at it and steps up nothing.
    let (_dir, world) = sample("fly-clear-block");
    let mut ed = editor(&world);
    block(&mut ed, 22.0, 3);
    over_the_ground(&mut ed, 16.0, 0.5);
    let high = g + FLY_RADIUS + 0.5;
    let into = fly_north(&mut ed, 36);
    for p in &into {
        assert!((p.z - high).abs() < 1e-4, "its height unchanged: {p:?}");
        assert!(
            p.y < 21.0 - FLY_RADIUS + 1e-3,
            "never into the block: {p:?}"
        );
    }
}
