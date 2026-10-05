//! Frame generators for the building-boundary stations: each writes PNGs of the recorded pose,
//! the room behind the house's front door, the door station and the door on/off differential to
//! `DERETH_TEST_BUILDING_BOUNDARY_DUMP` (the recorded-pose frame to
//! `DERETH_TEST_BUILDING_BOUNDARY_OUT`); with neither set they go to `building-boundary/` in
//! cargo's scratch folder for integration tests. They assert nothing; the claims are in
//! `rendering::building_boundary_draw`, whose fixture these share.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use dereth_world_data::env_cells::EnvCellLoader;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

use crate::rendering::building_boundary_draw::{
    door_shot, store, DOORWAY, DOOR_ROT, H, HOLTBURG, RECORDED_CELL, RECORDED_ORIGIN, RECORDED_ROT,
    W,
};

/// Where a frame goes when no variable names a place: cargo's scratch folder for integration
/// tests, inside its target directory.
fn default_dir() -> &'static str {
    env!("CARGO_TARGET_TMPDIR")
}

#[test]
#[ignore = "evidence generator: writes PNGs, run with DERETH_TEST_BUILDING_BOUNDARY_DUMP set and --ignored"]
fn render_the_recorded_pose_frame() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        time_of_day: Some(0.35),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(
            CellId(RECORDED_CELL),
            Frame::new(RECORDED_ORIGIN, RECORDED_ROT),
        ));

    let mut stream = ObjectStream::new();
    let mut now = 0.0f64;
    let mut rgba = Vec::new();
    for _ in 0..6 {
        now += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
        scene.stream(&store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    let settled = scene.character.as_ref().expect("a body").position();
    eprintln!(
        "the body settled in {:#010X} at [{:.3} {:.3} {:.3}], outdoors={}",
        settled.cell.0,
        settled.frame.origin.x,
        settled.frame.origin.y,
        settled.frame.origin.z,
        dereth_physics::landdefs::is_outdoors(settled.cell)
    );
    let path = std::env::var("DERETH_TEST_BUILDING_BOUNDARY_OUT").unwrap_or_else(|_| {
        format!(
            "{}/building-boundary/recorded-pose-frame.png",
            default_dir()
        )
    });
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).expect("the png's folder");
    }
    let f = std::fs::File::create(&path).expect("create the png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(&rgba).expect("png data");
    eprintln!("wrote {path}");
}

/// A point the body can stand at inside `cell` — the same search `rendering/indoor_depth_clear.rs` uses.
fn point_in(store: &RetailDatStore, cell: u32) -> Option<Vec3> {
    #[allow(clippy::cast_possible_truncation)]
    let block = (cell >> 16) as u16;
    let d = EnvCellLoader::new()
        .load_block(store, block)
        .into_iter()
        .find(|d| d.id.0 == cell)?;
    let g = dereth_world_data::env_cells::physics_geometry(&d);
    let bsp = g.cell_bsp.as_ref()?;
    for zi in -24i32..=24 {
        for i in -40i32..=40 {
            for j in -40i32..=40 {
                #[allow(clippy::cast_precision_loss)]
                let local = Vec3::new(i as f32 * 0.5, j as f32 * 0.5, zi as f32 * 0.5);
                if !bsp.point_inside_cell_bsp(local) {
                    continue;
                }
                if g.physics_bsp
                    .as_ref()
                    .is_some_and(|b| b.point_intersects_solid(local))
                {
                    continue;
                }
                return Some(dereth_physics::math::localtoglobal(&g.frame, local));
            }
        }
    }
    None
}

fn shot(
    store: &Arc<RetailDatStore>,
    cell: u32,
    origin: Vec3,
    yaw_deg: f32,
    tag: &str,
) -> Option<()> {
    let mut gpu = crate::common::test_gpu(W, H);
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        time_of_day: Some(0.35),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    let yaw = yaw_deg.to_radians();
    let q = Quat::new(math::cosf(yaw * 0.5), 0.0, 0.0, math::sinf(yaw * 0.5));
    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(CellId(cell), Frame::new(origin, q)));
    let mut stream = ObjectStream::new();
    let mut now = 0.0f64;
    let mut rgba = Vec::new();
    for _ in 0..6 {
        now += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(store, &mut gpu, &mut stream)
            .expect("sync_objects");
        // Follow the application's object-create integration: `sync_objects` prepares the
        // scene object, then `sync_physics_at` places its physics body. Without the latter,
        // rendering would use the wire cell instead of the body's achieved cell.
        if let Some(c) = scene.character.as_mut() {
            stream.sync_physics_at(store, &mut c.world, LocalTime(now));
        }
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
        // Follow the application frame order: `crate::camera::update_viewer` sweeps the camera
        // after the world update and records `CameraControl::viewer_cell`. If the viewer update
        // is omitted here, `WorldScene::viewer_cell` falls back to the body's cell, which
        // measures the doorway from the body rather than from the swept camera.
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            dereth_client_runtime::camera::CameraInput::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    let settled = scene.character.as_ref().expect("a body").position();
    eprintln!(
        "{tag}: settled {:#010X} [{:.3} {:.3} {:.3}] outdoors={}",
        settled.cell.0,
        settled.frame.origin.x,
        settled.frame.origin.y,
        settled.frame.origin.z,
        dereth_physics::landdefs::is_outdoors(settled.cell)
    );
    let Ok(dir) = std::env::var("DERETH_TEST_BUILDING_BOUNDARY_DUMP") else {
        return Some(());
    };
    let path = format!("{dir}/{tag}.png");
    write_png(&path, &rgba);
    eprintln!("{tag}: wrote {path}");
    Some(())
}

#[test]
#[ignore = "evidence generator: writes PNGs, run with DERETH_TEST_BUILDING_BOUNDARY_DUMP set and --ignored"]
fn probe_room_views() {
    let store = store();
    let p = point_in(&store, 0xA9B4_0143).expect("the ground floor has a standable point");
    eprintln!(
        "ground floor standable point [{:.3} {:.3} {:.3}]",
        p.x, p.y, p.z
    );
    for yaw in [0.0f32, 90.0, 180.0, 270.0] {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let tag = format!("room-yaw{}", yaw as i32);
        if shot(&store, 0xA9B4_0143, p, yaw, &tag).is_none() {
            return;
        }
    }
}

#[test]
#[ignore = "evidence generator: writes PNGs, run with DERETH_TEST_BUILDING_BOUNDARY_DUMP set and --ignored"]
fn probe_door_station() {
    let store = store();
    let door = Position::new(CellId(RECORDED_CELL), Frame::new(DOORWAY, DOOR_ROT));
    // Inside the room, near the doorway, looking west at it.
    for (tag, body, yaw) in [
        ("door-w", Vec3::new(138.20, 5.15, 94.00), 90.0f32),
        ("door-wnw", Vec3::new(138.20, 5.15, 94.00), 115.0),
        ("door-sw", Vec3::new(138.60, 7.60, 94.00), 135.0),
        ("door-s", Vec3::new(138.60, 8.60, 94.00), 170.0),
    ] {
        if door_shot(&store, 0xA9B4_0143, body, yaw, Some(door), tag).is_none() {
            return;
        }
    }
}

fn write_png(path: &str, rgba: &[u8]) {
    let f = std::fs::File::create(path).expect("create the png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().expect("png header");
    w.write_image_data(rgba).expect("png data");
}

#[test]
#[ignore = "evidence generator: writes PNGs, run with DERETH_TEST_BUILDING_BOUNDARY_DUMP set and --ignored"]
fn probe_door_differential() {
    let store = store();
    let door = Position::new(CellId(RECORDED_CELL), Frame::new(DOORWAY, DOOR_ROT));
    let dir = std::env::var("DERETH_TEST_BUILDING_BOUNDARY_DUMP")
        .unwrap_or_else(|_| format!("{}/building-boundary", default_dir()));
    std::fs::create_dir_all(&dir).expect("the dump folder");
    for (tag, body, yaw) in [
        ("dif-s", Vec3::new(138.60, 8.60, 94.00), 170.0f32),
        ("dif-wnw", Vec3::new(138.20, 5.15, 94.00), 115.0),
    ] {
        let Some(with) = door_shot(
            &store,
            0xA9B4_0143,
            body,
            yaw,
            Some(door),
            &format!("{tag}-with"),
        ) else {
            return;
        };
        let Some(without) = door_shot(
            &store,
            0xA9B4_0143,
            body,
            yaw,
            None,
            &format!("{tag}-without"),
        ) else {
            return;
        };
        let mut diff = vec![0u8; with.len()];
        let mut n = 0usize;
        for (i, (a, b)) in with
            .chunks_exact(4)
            .zip(without.chunks_exact(4))
            .enumerate()
        {
            let changed = a[..3] != b[..3];
            if changed {
                n += 1;
            }
            let px = &mut diff[i * 4..i * 4 + 4];
            px[0] = if changed { 255 } else { b[0] / 3 };
            px[1] = if changed { 0 } else { b[1] / 3 };
            px[2] = if changed { 0 } else { b[2] / 3 };
            px[3] = 255;
        }
        eprintln!("{tag}: {n} px changed by the door");
        write_png(&format!("{dir}/{tag}-diff.png"), &diff);
    }
}

/// **The recorded station.** The body stands in the house's front doorway, which is the
/// **outdoor** landcell `0xA9B40029`, while its camera, three metres behind, is inside the room.
#[test]
#[ignore = "evidence generator: writes PNGs, run with DERETH_TEST_BUILDING_BOUNDARY_DUMP set and --ignored"]
fn probe_body_in_the_doorway() {
    let store = store();
    let door = Position::new(CellId(RECORDED_CELL), Frame::new(DOORWAY, DOOR_ROT));
    // yaw 90 deg = facing west, out of the house, so the camera is swept back into the room.
    // yaw 270 deg = facing east, into the house, so the camera is outside: the control.
    for (tag, yaw) in [
        ("doorway-camera-looking-inside", 90.0f32),
        ("doorway-camera-looking-outside", 270.0),
    ] {
        if door_shot(&store, RECORDED_CELL, RECORDED_ORIGIN, yaw, Some(door), tag).is_none() {
            return;
        }
    }
}
