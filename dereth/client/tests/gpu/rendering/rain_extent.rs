//! The rain falls across the whole view, not only down to the horizon. Retail's falling drops,
//! measured from a standing character in Shoushi on a "Rainy" day, span rows 30 to 565 of a
//! 629-row window with the horizon near row 185.
//!
//! The sky update pins a weather object to the viewer in `x` and `y` and puts its origin at the
//! absolute `z` of -120.0 unless the object's `properties` bit 3 is set; the rain curtain's own
//! geometry runs `z ∈ [0.1, 814.9]` with nothing below its origin, so that floor is what carries
//! the rain below the horizon (`dereth_client::sky`'s `WEATHER_FLOOR_Z`).
//! Fixture: the retail dats and a software device, drawn at midday of the offline clock's
//! "Rainy" day group.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::{SceneConfig, WorldScene};
use dereth_render::device::Gpu;

const W: u32 = 640;
const H: u32 = 480;

fn warp() -> Gpu {
    crate::common::software_gpu(W, H)
}

fn draw(scene: &WorldScene, gpu: &mut Gpu) -> Vec<u8> {
    scene.reserve_upload_arena(gpu).expect("reserve");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

/// Behaviour: rendering.weather.rain-falls-across-the-whole-view
/// The weather layer reaches the bottom of the frame, as it does in retail.
///
/// The measurement is a differential: the same frame drawn with weather enabled and
/// disabled. The sky builds no object for a `properties & 4` entry while the flag is clear, so
/// every pixel that changes between the two is the weather layer and nothing else — no time
/// passes, nothing else in the scene moves.
///
/// A curtain placed at the viewer's eye instead of the floor stops exactly at the horizon of a
/// level camera: the raindrops would not reach beyond halfway down the screen.
#[test]
fn the_weather_layer_falls_across_the_whole_view() {
    // The dats are the oracle and are not optional, so their absence fails; the device is
    // optional, so only its absence returns early.
    let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {} -- \
             set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    });
    let mut gpu = warp();
    let store = std::sync::Arc::new(store);
    let cfg = SceneConfig {
        land_radius: 2,
        scenery_radius: 0,
        character: false,
        particles: false,
        building_portals: false,
        // Midday is inside the rain objects' own time window. The offline
        // clock starts at year 10 day 0, which the day-group selection hashes to group 3, "Rainy".
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("loads");
    let (y, d, t) = scene.game_time();
    let region = dereth_client::world::load_region(&store).expect("region");
    let group = dereth_world_render::sky::present_day_group(&region, y, d).expect("group");
    assert_eq!(
        group.day_name, "Rainy",
        "year {y} day {d} t {t} must be a weather-carrying group"
    );

    // A level camera, so "halfway down the screen" is the horizon and the assertion below is a
    // statement about the horizon rather than about this particular pitch.
    scene.camera.pitch = 0.0;
    scene.camera.yaw = 0.0;
    scene.camera.position.z -= 40.0;

    let step = |scene: &mut WorldScene| {
        scene.update(
            dereth_client::camera::CameraInput::default(),
            dereth_client::character::CharacterInput::default(),
            dereth_primitives::LocalTime(0.0),
            0.0,
        );
    };

    scene.set_weather_enabled(true);
    step(&mut scene);
    let with_weather = scene.draw.stats.sky_objects;
    let on = draw(&scene, &mut gpu);
    scene.set_weather_enabled(false);
    step(&mut scene);
    let without = scene.draw.stats.sky_objects;
    let off = draw(&scene, &mut gpu);
    assert!(
        with_weather > without,
        "the toggle must remove objects: {with_weather} vs {without}"
    );

    let mut rows: Vec<u32> = vec![0; H as usize];
    for (yy, row) in rows.iter_mut().enumerate() {
        for xx in 0..W as usize {
            let i = (yy * W as usize + xx) * 4;
            if on[i..i + 3] != off[i..i + 3] {
                *row += 1;
            }
        }
    }
    let total: u32 = rows.iter().sum();
    let first = rows.iter().position(|&c| c > 0);
    let last = rows.iter().rposition(|&c| c > 0);
    eprintln!("{total} weather pixels; rows {first:?} ..= {last:?} of {H}");
    assert!(
        total > 1000,
        "the weather layer must be visible at all: {total} pixels"
    );

    let mid = H as usize / 2;
    let below: u32 = rows[mid..].iter().sum();
    let bottom_eighth: u32 = rows[H as usize * 7 / 8..].iter().sum();
    assert!(
        below > 0 && bottom_eighth > 0,
        "the weather layer must reach past the horizon and into the bottom of the frame, as it \
         does in retail (rows 30..565 of 629): below the midline {below}, bottom eighth \
         {bottom_eighth}, rows {first:?}..={last:?}"
    );
}

/// Why the origin's `z` decides the whole question: the rain curtain has **no geometry below its
/// own origin**, so wherever the origin is put is where the rain stops.
///
/// Oracle: the shipped `client_portal.dat`. Region 1's "Rainy" groups name gfx `0x01004C42` and
/// `0x01004C44` for their `properties & 4` objects, and both are 48-vertex meshes spanning
/// `x, y ∈ [-113.5, 113.5]`, `z ∈ [0.1, 814.9]`.
#[test]
fn the_rain_curtain_has_nothing_below_its_own_origin() {
    // The dats are the oracle: without them this fails rather than passing on no mesh.
    let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {} -- \
             set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    });
    let region = dereth_client::world::load_region(&store).expect("region");
    let info = region.sky_info.as_ref().expect("sky info");
    let mut checked = 0usize;
    for group in &info.day_groups {
        for t in [0.25_f32, 0.5, 0.75] {
            for p in dereth_world_render::sky::get_sky(group, t) {
                // Bit 3 exempts an object from the -120 floor; the only one that sets it in the
                // shipped data is the rainy groups' star field, which has no drawable geometry.
                if p.properties & 4 == 0 || p.properties & 8 != 0 || p.gfx_id.0 == 0 {
                    continue;
                }
                for part in dereth_client::models::resolve_parts(&store, p.gfx_id) {
                    let groups = dereth_client::models::build_gfxobj(&store, part.gfxobj);
                    let mut lo = f32::MAX;
                    let mut hi = f32::MIN;
                    for g in &groups {
                        for (v, _, _) in &g.vertices {
                            lo = lo.min(v.z + part.frame.origin.z);
                            hi = hi.max(v.z + part.frame.origin.z);
                        }
                    }
                    assert!(
                        lo >= 0.0,
                        "gfx {:#010x} reaches below its origin: {lo}",
                        p.gfx_id.0
                    );
                    assert!(
                        hi > 100.0,
                        "gfx {:#010x} is not a tall curtain: {hi}",
                        p.gfx_id.0
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 0, "no weather geometry found in any day group");
    eprintln!("{checked} weather parts checked");
}
