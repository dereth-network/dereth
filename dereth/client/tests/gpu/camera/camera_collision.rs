//! The swept camera does not pass through walls or sink into hillsides, settles at the shipped
//! third-person offset, and routes mouse-look through the five-sample filter. Wall and hill claims
//! are differential: two camera managers follow the identical pivot stream, one through the
//! collision sweep and one straight out of the smoother, and the claim is that the swept one stays
//! where the unswept one leaves. Fixture: Holtburg's interior cells and terrain from the retail
//! `client_cell_1.dat` and `client_portal.dat`, judged by each room's own cell and physics BSPs,
//! on a software GPU device. Fails when the dats or the device are absent.

#![cfg(gpu)]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_dat::RetailDatStore;
use dereth_physics::math::V3;
use dereth_physics::source::EnvCellGeometry;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use {
    dereth_client_runtime::camera::pivot_state, dereth_client_runtime::camera::CameraManager,
    dereth_client_runtime::camera::CameraState, dereth_client_runtime::camera::CameraTick,
};
use {
    dereth_client_runtime::character::CharacterInput,
    dereth_client_runtime::character::PLAYER_OBJECT_ID,
};
use {
    dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene,
    dereth_world_data::landblock::DEFAULT_LANDBLOCK,
};
use {dereth_world_data::env_cells::physics_geometry, dereth_world_data::env_cells::EnvCellLoader};

/// The camera's per-tick inputs (current time, frame rate, the mouse-turning preference),
/// supplied as the frame loop supplies them.
fn tick(now: f64) -> CameraTick {
    CameraTick {
        now,
        fps: 30.0,
        mouse_turning: false,
        player_heading: Some(0.0),
    }
}

/// The retail store, or **fail**: an absent dat is a failure, never a skip.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Every Holtburg interior cell that has a standable point, with that point in **block-local**
/// space and the cell's own geometry alongside, so the test can ask the room where a point is.
///
/// The same search the `interiors` tests use: accept a candidate only when the
/// cell BSP contains it and the physics BSP does not place it inside a solid.
fn rooms(store: &RetailDatStore, want: usize) -> Vec<(Vec3, EnvCellGeometry)> {
    let mut out = Vec::new();
    let mut loader = EnvCellLoader::new();
    for d in loader.load_block(store, DEFAULT_LANDBLOCK) {
        if out.len() >= want {
            break;
        }
        let g = physics_geometry(&d);
        let Some(bsp) = g.cell_bsp.as_ref() else {
            continue;
        };
        'cell: for &z in &[0.5f32, 1.0, 1.5] {
            for i in -6i8..=6 {
                for j in -6i8..=6 {
                    let local = Vec3::new(f32::from(i) * 0.5, f32::from(j) * 0.5, z);
                    if !bsp.point_inside_cell_bsp(local) {
                        continue;
                    }
                    if g.physics_bsp
                        .as_ref()
                        .is_some_and(|b| b.point_intersects_solid(local))
                    {
                        continue;
                    }
                    let world = dereth_physics::math::localtoglobal(&g.frame, local);
                    out.push((world, g.clone()));
                    break 'cell;
                }
            }
        }
    }
    out
}

/// Resolve the point's actual cell and ask whether it is still inside the building.
fn indoors(world: &dereth_physics::PhysicsWorld, p: Position) -> bool {
    world
        .adjust_position(&p, Vec3::ZERO)
        .and_then(|(_, c)| c)
        .is_some_and(|id| !dereth_physics::landdefs::is_outdoors(id))
}

/// A `Position` expressed in `at`'s landblock, then in `g`'s own cell space — which is the space
/// the room's BSPs are in.
fn in_cell_space(g: &EnvCellGeometry, at: CellId, p: Position) -> Vec3 {
    let world = dereth_physics::landdefs::get_block_offset(at, p.cell).add(p.frame.origin);
    dereth_physics::math::globaltolocal(&g.frame, world)
}

/// Behaviour: camera.wall.the-swept-camera-stays-in-the-room
/// Oracle: the retail room's own cell BSP, run over two cameras driven from the identical stream
/// of pivot positions.
///
/// The control uses the smoother alone, with no sweep, as a debug chase camera would. The
/// subject uses the same manager with a swept sphere placing the eye each frame.
///
/// Both are seeded from the same pivot and fed the same current time; the *only* difference
/// is the collision sweep. What is asserted is the difference: in a room whose walls are
/// behind the camera, the control ends outside the room and the subject does not.
#[test]
fn the_swept_camera_stays_in_the_room_the_unswept_one_leaves() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let found = rooms(&store, 20);
    assert!(
        found.len() >= 20,
        "only {} standable rooms in Holtburg",
        found.len()
    );

    // Reuse one scene, teleporting between trials. Creating a scene per trial exhausted the
    // D3D12 backend's 2,048-entry texture descriptor heap; the interiors tests also reuse it.
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a fixed simulation step in seconds
    let dtf = step as f32;

    let mut escaped_without_the_sweep = 0usize;
    let mut held_by_the_sweep = 0usize;
    let mut solid_with_the_sweep = 0usize;
    let mut trials = 0usize;

    for (inside, g) in &found {
        for quarter in 0..4u32 {
            #[allow(clippy::cast_precision_loss)] // 0..4
            let half = (quarter as f32) * std::f32::consts::FRAC_PI_4;
            let heading = Quat::new(math::cosf(half), 0.0, 0.0, math::sinf(half));
            {
                let c = scene.character.as_mut().expect("a body");
                c.teleport(Position::new(g.id, Frame::new(*inside, heading)));
            }
            scene.follow_character_now();

            // The control: a second `CameraManager` configured exactly as the live one is, driven
            // from the same pivots, whose output feeds straight back in with no sweep between.
            let mut free_cm = CameraManager::new();
            let _free_set = CameraState::new(&mut free_cm, PLAYER_OBJECT_ID, tick(0.0));
            let mut free_pos = {
                let c = scene.character.as_ref().expect("a body");
                let p = pivot_state(&c.world, c.handle).expect("the body is in the arena");
                free_cm.query_pivot_position(&p)
            };

            // Stand still and let the camera settle: the smoother closes 45% of the gap per
            // 100 ms, so a second is far more than enough to reach the full 2.5 m of set-back.
            let mut now = 0.0f64;
            for _ in 0..60 {
                now += step;
                scene.update(
                    dereth_client_runtime::camera::CameraInput::default(),
                    CharacterInput::default(),
                    LocalTime(now),
                    dtf,
                );
                dereth_client_runtime::camera::update_viewer(
                    &mut scene,
                    dereth_client_runtime::camera::CameraInput::default(),
                    LocalTime(now),
                    step,
                );
                let c = scene.character.as_ref().expect("a body");
                let p = pivot_state(&c.world, c.handle).expect("the body is in the arena");
                free_pos = free_cm.update_camera(free_pos, &p, None, now);
            }

            let c = scene.character.as_ref().expect("a body");
            let swept_local = in_cell_space(g, g.id, c.camera.viewer);

            trials += 1;
            // Resolve each camera's containing cell. For an interior start, the search walks the
            // room and its stab list and only falls out to the land cell when the point is in
            // none of them. Sliding through a doorway into the next room is therefore *not*
            // counted as an escape, which is right: it is not passing through a wall.
            if !indoors(&c.world, free_pos) {
                escaped_without_the_sweep += 1;
                if c.camera
                    .viewer_cell
                    .is_some_and(|id| !dereth_physics::landdefs::is_outdoors(id))
                {
                    held_by_the_sweep += 1;
                }
            }
            // Only meaningful while the camera is still in *this* room: a cell's physics BSP
            // describes the walls it owns, and a point in the room next door is legitimately
            // inside them.
            if c.camera.viewer_cell == Some(g.id)
                && g.physics_bsp
                    .as_ref()
                    .is_some_and(|b| b.point_intersects_solid(swept_local))
            {
                solid_with_the_sweep += 1;
            }
        }
    }

    // The unswept camera is 2.5 m behind a body standing in a room a few metres across, so it is
    // outside the room most of the time. If it were not, there would be nothing to test.
    eprintln!(
        "{trials} trials: the unswept control left the building on {escaped_without_the_sweep}, \
         and on {held_by_the_sweep} of those the sweep kept the camera inside; \
         {solid_with_the_sweep} swept positions ended in solid geometry"
    );
    assert!(
        escaped_without_the_sweep * 4 >= trials,
        "the control only left the building on {escaped_without_the_sweep} of {trials} trials, so \
         this differential is not measuring anything"
    );
    // And the sweep holds it. Not all of them, and it should not be all: a room with an open
    // doorway behind the camera is a room the camera is *allowed* to back out of, and a covered
    // walkway has an opening on every side. Three quarters is far past what a camera that ignored
    // geometry could reach, because the control's own number is what it would score.
    assert!(
        held_by_the_sweep * 4 >= escaped_without_the_sweep * 3,
        "the sweep held the camera inside on only {held_by_the_sweep} of the \
         {escaped_without_the_sweep} trials the unswept camera left the building"
    );
    // Whatever it did, it never ended inside a wall. This one is absolute: `find_valid_position`
    // returning a point inside solid geometry is a physics failure, not a camera judgement call.
    assert_eq!(
        solid_with_the_sweep, 0,
        "the swept camera ended inside solid geometry on {solid_with_the_sweep} of {trials} trials"
    );

    let c = scene.character.as_ref().expect("a body");
    assert!(c.camera.stats.sweeps > 0, "no sweep ran at all");
    assert!(
        c.camera.stats.sweeps_blocked > 0,
        "no sweep was ever shortened by geometry, so the sphere is not meeting the walls"
    );
}

/// Oracle: the physics crate's terrain collision, over the retail landblock. A hillside is a
/// different code path from a wall: the ground is a land cell's two triangles, not an interior
/// BSP.
///
/// The body walks across Holtburg's terrain with the camera behind it; the assertion is that the
/// camera's own sphere never sinks below the terrain under it, and that the unswept control does.
#[test]
fn the_swept_camera_does_not_sink_into_a_hillside() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a fixed simulation step in seconds
    let dtf = step as f32;
    let radius = dereth_physics::globals::VIEWER_SPHERE_RADIUS;
    let input = CharacterInput {
        forward: true,
        run: true,
        ..CharacterInput::default()
    };

    // Zoom out and lower the eye first, through the camera's own actions. The shipped default sits 0.75
    // above a pivot 1.5 above the feet and 2.5 back, which on Holtburg's gentle terrain never
    // reaches the ground; a player who has zoomed out and looked down is dragging the eye through
    // the hill behind him, and that is the case the sweep exists for. Both limits still hold:
    // |y| <= 10 out, and the lowering action's refusal band is 1.2 m wide.
    let mut now = 0.0f64;
    // A few frames first, so the measured frame rate is non-zero: the three rate limiters
    // seed themselves from it and a zero frame rate makes the first step degenerate.
    for _ in 0..5 {
        now += step;
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            input,
            LocalTime(now),
            dtf,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(now),
            step,
        );
    }
    {
        let c = scene.character.as_mut().expect("a body");
        now += 1.0;
        c.camera.on_action(
            dereth_client_runtime::actions::camera::CameraCommand::Farther { extent: 10.0 },
            PLAYER_OBJECT_ID,
            now,
        );
        // Lowering swings by `angle * (dt * adjustment_speed)` radians, so it is driven at
        // the frame rate the rest of the test uses rather than in one-second jumps -- a step of a
        // whole second is 320 degrees and spins the offset right round.
        for _ in 0..10 {
            if c.camera.manager.viewer_offset.z < -3.0 {
                break;
            }
            now += step;
            c.camera.on_action(
                dereth_client_runtime::actions::camera::CameraCommand::Lower { extent: 1.0 },
                PLAYER_OBJECT_ID,
                now,
            );
        }
        assert!(
            c.camera.manager.viewer_offset.z < -3.0,
            "the eye is not far enough below the pivot: {:?}",
            c.camera.manager.viewer_offset
        );
    }
    let offset = scene
        .character
        .as_ref()
        .expect("a body")
        .camera
        .manager
        .viewer_offset;

    let mut free_cm = CameraManager::new();
    let _free_set = CameraState::new(&mut free_cm, PLAYER_OBJECT_ID, tick(0.0));
    // The control gets the identical offset, so the only difference is still the sweep.
    free_cm.viewer_offset = offset;
    let mut free_pos = {
        let c = scene.character.as_ref().expect("a body");
        let p = pivot_state(&c.world, c.handle).expect("the body is in the arena");
        free_cm.query_pivot_position(&p)
    };

    let mut swept_below = 0usize;
    let mut free_below = 0usize;
    let mut samples = 0usize;
    let mut min_swept = f32::MAX;
    let mut min_free = f32::MAX;
    let mut walked = 0.0f32;
    let mut prev = scene
        .character
        .as_ref()
        .expect("a body")
        .position()
        .frame
        .origin;
    for i in 0..600 {
        now += step;
        // Turn every hundred steps, so the walk crosses slopes in several directions rather than
        // running along one contour.
        let mut walk = input;
        if i % 100 < 20 {
            walk.turn_left = true;
        }
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            walk,
            LocalTime(now),
            dtf,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(now),
            step,
        );
        let c = scene.character.as_ref().expect("a body");
        let p = pivot_state(&c.world, c.handle).expect("the body is in the arena");
        free_pos = free_cm.update_camera(free_pos, &p, None, now);
        let here = c.position().frame.origin;
        walked += Vec3::new(here.x - prev.x, here.y - prev.y, 0.0)
            .mag2()
            .sqrt();
        prev = here;

        // `terrain_height_at` evaluates the landblock's terrain plane and requires the cell
        // to agree with the origin. Resolve each camera's cell first: smoothing preserves
        // the input cell ID, so the unswept control still carries its initial pivot cell
        // and could otherwise produce no terrain-height answer after crossing a boundary.
        let ground = |pos: Position| -> Option<(f32, f32)> {
            let (adj, cell) = c.world.adjust_position(&pos, Vec3::ZERO)?;
            if !dereth_physics::landdefs::is_outdoors(cell?) {
                return None;
            }
            c.world
                .terrain_height_at(&adj)
                .map(|h| (h, adj.frame.origin.z))
        };
        if let (Some((gs, zs)), Some((gf, zf))) = (ground(c.camera.viewer), ground(free_pos)) {
            samples += 1;
            // "Below the terrain" for a sphere is *touching* it: a centre less than one radius
            // above the ground has the sphere in the dirt, and the sweep's own answer comes to
            // rest exactly one radius clear. The tolerance is float slack on that tangency.
            min_swept = min_swept.min(zs - gs);
            min_free = min_free.min(zf - gf);
            if zs < gs + radius - 1e-3 {
                swept_below += 1;
            }
            if zf < gf + radius - 1e-3 {
                free_below += 1;
            }
        }
    }

    eprintln!(
        "{samples} samples: the sweep was in the ground on {swept_below} and the control on \
         {free_below}; closest clearance {min_swept:.3} m swept, {min_free:.3} m unswept, \
         against the sphere's own {radius} m radius"
    );
    assert!(
        samples > 300,
        "only {samples} of 600 steps produced a terrain height"
    );
    // And the body walked. The camera sweep initializes its collision transition with the player,
    // so a stop-velocity result from the *camera's* sweep zeroes the **player's** velocity.
    // The original client also does this; animation-driven movement restores velocity each frame.
    // Dragging the eye through the ground for twenty seconds would expose a failure to restore it,
    // so the distance is asserted rather than assumed.
    assert!(
        walked > 40.0,
        "the body only covered {walked:.1} m in twenty seconds: the camera's sweep is \
         killing its velocity"
    );
    // The differential: the unswept camera dips under the ground when the body runs downhill or
    // stands on a rise, and the swept one does not. If the control never dipped there would be
    // nothing to prove, so both halves are asserted.
    assert!(
        free_below > 0,
        "the unswept control never went below the terrain, so this walk crosses no slope steep \
         enough to test"
    );
    assert_eq!(
        swept_below, 0,
        "the swept camera was below the terrain on {swept_below} of {samples} samples, where the \
         unswept control was on {free_below}"
    );
}

/// Oracle: the original third-person default offsets, multiplied by the camera scale.
///
/// The shipped third-person camera is 2.5 units behind and 0.75 above a pivot 1.5 units above the
/// player's origin. That is a distance from the body, and the assertion is on the *placed* camera
/// rather than on the offset — the offset is what the model says and the placement is what the
/// player sees.
///
/// The default eye offset is `(0, -2.5 * s, 0.75 * s)`, and the gameplay UI sets camera scale
/// `s` to **1.1** every frame it is active. This test drives the frame loop, so it sees the
/// scaled offset the player actually gets — and that offset is what carries `LOOK_AT_PIVOT` and
/// the camera's 16.7-degree pitch; see `camera::camera_pitch`.
#[test]
fn the_settled_camera_sits_at_the_shipped_third_person_offset() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a fixed simulation step in seconds
    let dtf = step as f32;
    let mut now = 0.0f64;
    for _ in 0..120 {
        now += step;
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            dtf,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(now),
            step,
        );
    }

    let c = scene.character.as_ref().expect("a body");
    let s = dereth_client_runtime::camera::GAMEPLAY_CAMERA_SCALE;
    let shipped = Vec3::new(0.0, -2.5 * s, 0.75 * s);
    assert_eq!(c.camera.manager.viewer_offset, shipped);
    assert_eq!(
        c.camera.manager.pivot_offset,
        Vec3::new(0.0, 0.0, 1.5),
        "the pivot is unscaled"
    );

    // The eye relative to the pivot, in the pivot's own frame. Standing on open ground with the
    // sweep finding nothing, it must be the offset itself.
    let p = pivot_state(&c.world, c.handle).expect("the body is in the arena");
    let pivot = c.camera.manager.query_pivot_position(&p);
    let d = dereth_physics::math::get_offset(&pivot, &c.camera.viewer);
    let dist = d.mag2().sqrt();
    let want = shipped.mag2().sqrt();
    assert!(
        (dist - want).abs() < 0.05,
        "the settled camera is {dist:.3} m from the pivot, not the shipped {want:.3} m"
    );
    // And it is *behind* and *above*: the client's +y is the body's forward.
    assert!(d.z > 0.5, "the camera is not above the pivot: {d:?}");
}

/// Behaviour: camera.mouse-look.only-moves-from-the-sixth-frame
/// Oracle: the five-sample mouse-look filter, `dereth_client_runtime::actions::camera::MouseLook`,
/// which its own crate tests directly.
///
/// What this asserts is that the scene's camera *routes* mouse-look through it rather than
/// reimplementing it: the camera must not move for five consecutive samples on an axis and must
/// move on the sixth. A camera that applied the delta itself would move on the first.
#[test]
fn mouse_look_only_moves_the_camera_from_the_sixth_frame() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    // Settle first, so the offset under test is the shipped one.
    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a fixed simulation step in seconds
    let dtf = step as f32;
    let mut now = 0.0f64;
    for _ in 0..120 {
        now += step;
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            dtf,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(now),
            step,
        );
    }

    let before = scene
        .character
        .as_ref()
        .expect("a body")
        .camera
        .manager
        .viewer_offset;
    for i in 1..=5 {
        now += step;
        dereth_client_runtime::camera::mouse_look(&mut scene, 60.0, 0.0, LocalTime(now));
        let off = scene
            .character
            .as_ref()
            .expect("a body")
            .camera
            .manager
            .viewer_offset;
        assert_eq!(
            off, before,
            "the camera moved on mouse sample {i}, inside the dead zone"
        );
    }
    now += step;
    dereth_client_runtime::camera::mouse_look(&mut scene, 60.0, 0.0, LocalTime(now));
    let after = scene
        .character
        .as_ref()
        .expect("a body")
        .camera
        .manager
        .viewer_offset;
    assert_ne!(after, before, "the sixth sample did not reach the camera");
    // `Rotate` swings the offset about the pivot: its length is preserved.
    assert!(
        (after.mag2().sqrt() - before.mag2().sqrt()).abs() < 1e-4,
        "Rotate changed the camera's distance: {before:?} -> {after:?}"
    );
}

/// Oracle: the rendered frame and the environment cell's visible contents. This is the
/// end-to-end version of the first test and the one a person can check by
/// looking: stand in a Holtburg room and draw a frame with **no hand-placed camera**.
///
/// Placement is left to the production camera update, with no camera pulled onto the body: a
/// failed sweep would show the street instead of the room.
///
/// Set `DERETH_TEST_CAMERA_COLLISION_DUMP` to a path to have the frame written there as raw RGBA
/// (width and height on the first line of a `.txt` beside it), to check it by looking.
#[test]
fn a_frame_from_inside_a_holtburg_room_is_of_the_room() {
    let store = store();
    let mut gpu = crate::common::test_gpu(800, 600);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let (inside, g) = rooms(&store, 1)
        .into_iter()
        .next()
        .expect("Holtburg has an interior cell");

    let mut scene =
        WorldScene::load(&store, &mut gpu, SceneConfig::default()).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    assert_eq!(scene.env_cell_counts().1, 0, "the body starts outdoors");

    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(g.id, Frame::new(inside, Quat::IDENTITY)));
    scene.follow_character_now();

    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a fixed simulation step in seconds
    let dtf = step as f32;
    let mut now = 0.0f64;
    for _ in 0..60 {
        now += step;
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            dtf,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(now),
            step,
        );
    }

    // The camera placed itself, and it placed itself in an interior cell.
    let c = scene.character.as_ref().expect("a body");
    let cell = c
        .camera
        .viewer_cell
        .expect("the sweep found no camera position at all");
    assert!(
        !dereth_physics::landdefs::is_outdoors(cell),
        "the camera ended outdoors in {cell:?} while the body stands in {:?}",
        g.id
    );
    assert!(
        scene.env_cell_counts().1 > 0,
        "the indoor draw path found no geometry for the viewer's cell"
    );

    gpu.begin_frame().expect("begin");
    scene.draw(&mut gpu).expect("draw");
    gpu.end_frame().expect("end");
    let image = gpu.capture().expect("capture");
    let rgba = image.to_rgba();
    if let Ok(path) = std::env::var("DERETH_TEST_CAMERA_COLLISION_DUMP") {
        std::fs::write(&path, &rgba).expect("dump");
        std::fs::write(
            format!("{path}.txt"),
            format!("{} {}\n", image.width, image.height),
        )
        .expect("dump");
    }
    // The same criterion the `interiors` tests use: a frame of the inside of a lit room is mostly
    // not black, where a frame of a wall from the wrong side of it is.
    let lit = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]) > 24)
        .count();
    let total = (image.width * image.height) as usize;
    assert!(
        lit * 2 > total,
        "only {lit} of {total} pixels are lit: the camera is not looking at the inside of the room"
    );
}
