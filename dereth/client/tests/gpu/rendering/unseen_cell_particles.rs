//! Particles in cells the viewer does not reach are not drawn: from outside a villa's front gate
//! the courtyard floor shows none of the basement's emitters, while the room a body stands in
//! keeps its candelabra particles.
//! Fixture: the villa on landblock 0x9DAF from the retail dats on a software device, 60 frames per
//! shot; no network. `DERETH_TEST_UNSEEN_CELL_PARTICLES_DUMP=<dir>` writes each shot as a PNG.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::camera::FreeCamera;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
use dereth_render::device::{DeviceConfig, Gpu};
use std::sync::Arc;

fn shot(particles: bool, inside: bool) -> Vec<u8> {
    const W: u32 = 800;
    const H: u32 = 600;
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: W,
            height: H,
            ..DeviceConfig::default()
        },
    )
    .expect("D3D12 WARP");
    let mut scene = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            landblock: 0x9DAF,
            character: inside,
            time_of_day: Some(0.35),
            particles,
            ..SceneConfig::default()
        },
    )
    .expect("recorded villa block");
    // The villa's front gate, viewed from outside through a constructed stationary camera; the
    // inside shot stands a body in the lit room 0x9DAF0127.
    if inside {
        let region = dereth_client::world::load_region(&store).expect("region");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("body");
        scene
            .character
            .as_mut()
            .expect("body")
            .teleport(Position::new(
                CellId(0x9DAF_0127),
                Frame::new(
                    Vec3::new(134.657_913, 34.905_537, 138.004_990),
                    Quat::new(0.499316, 0.0, 0.0, -0.866420),
                ),
            ));
    } else {
        scene.camera = FreeCamera::new(Vec3::new(145.4, 20.3, 140.2), 0.55, -0.18);
    }
    let mut objects = ObjectStream::new();
    for frame in 1..=60 {
        scene
            .sync_objects(&store, &mut gpu, &mut objects)
            .expect("hosts");
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(f64::from(frame) / 30.0),
            1.0 / 30.0,
        );
        if inside {
            dereth_client::camera::update_viewer(
                &mut scene,
                Default::default(),
                LocalTime(f64::from(frame) / 30.0),
                1.0 / 30.0,
            );
        }
        scene.stream(&store, &mut gpu).expect("stream");
        scene.reserve_upload_arena(&mut gpu).expect("arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
    }
    let probes = scene.emitter_degrade_probe();
    let basement = probes
        .iter()
        .filter(|p| p.origin.z < 137.0 && p.cypt < 40.0)
        .count();
    assert!(
        basement > 0,
        "the scene actually contains nearby basement emitters"
    );
    if inside {
        assert!(
            scene.drawn_cells().as_ref().unwrap().contains(&0x9DAF_0127),
            "the lit room is reached"
        );
    }
    eprintln!(
        "inside={inside} particles={particles} basement emitters={basement} draw={:?} cells={:?}",
        scene.drawn_particles(),
        scene.drawn_cells()
    );
    let rgba = gpu.capture().expect("pixels").to_rgba();
    if let Ok(dir) = std::env::var("DERETH_TEST_UNSEEN_CELL_PARTICLES_DUMP") {
        let path =
            std::path::Path::new(&dir).join(format!("inside-{inside}-particles-{particles}.png"));
        let mut png = png::Encoder::new(
            std::io::BufWriter::new(std::fs::File::create(path).expect("png")),
            W,
            H,
        );
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .expect("header")
            .write_image_data(&rgba)
            .expect("image");
    }
    rgba
}

#[test]
/// Behaviour: rendering.particles.emitters-in-unreached-cells-are-not-drawn
fn the_villa_courtyard_floor_does_not_show_unseen_basement_particles() {
    let off = shot(false, false);
    let on = shot(true, false);
    let mut changed = 0;
    for y in 260..450 {
        for x in 250..470 {
            let at = (y * 800 + x) * 4;
            if off[at..at + 4] != on[at..at + 4] {
                changed += 1;
            }
        }
    }
    eprintln!("courtyard-floor particle pixels={changed}");
    assert_eq!(
        changed, 0,
        "the courtyard floor cannot display emitters in unseen basement cells"
    );
}

#[test]
fn the_same_villas_visible_room_keeps_its_candelabra_particles() {
    let off = shot(false, true);
    let on = shot(true, true);
    let changed = off
        .chunks_exact(4)
        .zip(on.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    eprintln!("visible-room particle pixels={changed}");
    assert!(
        changed > 100,
        "a reached room's real candelabra must retain visible particles"
    );
}
