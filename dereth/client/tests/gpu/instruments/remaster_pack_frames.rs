//! An art pack drawn by the remaster at the places the pack's art shows best and worst: each
//! place drawn with nothing of the pack, with all of it, and with all of it and the water, the
//! ground as the player draws it (from its splats, which the pack's ground is laid behind); what
//! of the pack the remaster holds, and how long its frames took to come with it.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! - `DERETH_ART_PACK`: the pack's folder or packed file (required).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it).
//! - `DERETH_REMASTER_PACK_SPOTS`: a comma list of the places to draw (default all).
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::report::VisualReport;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{RemasterLight, RemasterPacks, RemasterWater};

use super::stations::{self, report_station, Shot};

/// The places, each as a report line says where its camera stood and when.
const SPOTS: &[(&str, &str)] = &[
    (
        "lily-pads",
        "dereth report 1; loc 0xA8B60012 [55.615215 30.300829 27.554998] 0.069254 0.000000 \
         0.000000 -0.997599; camera 0xA8B6 [54.51643 35.022827 32.58985] yaw -2.9129736 pitch \
         -0.6300002 cell 0xA8B60012; time 0.771; clock 306486294.7270536; weather auto",
    ),
    (
        "bushes-near",
        "dereth report 1; loc 0xA9B5001B [77.633850 61.303265 27.904999] -0.807961 0.000000 \
         0.000000 -0.589236; camera 0xA9B5 [83.12427 59.54088 31.063133] yaw -5.022987 pitch \
         -0.28000048 cell 0xA9B5001B; time 0.788; clock 306486420.1491092; weather auto",
    ),
    (
        "bush-close",
        "dereth report 1; loc 0xA9B50002 [2.799240 45.944492 28.238268] -0.819516 0.000000 \
         0.000000 0.573056; camera 0xA9B5 [-2.616928 43.965504 31.396402] yaw -1.2204808 pitch \
         -0.28000048 cell 0xA8B5003A; time 0.794; clock 306486468.70058435; weather auto",
    ),
    (
        "trees-in-water",
        "dereth report 1; loc 0xA9B5000D [39.936409 110.799110 27.554998] 0.614589 0.000000 \
         0.000000 -0.788848; camera 0xA9B5 [34.34513 112.209175 30.713133] yaw -1.8178362 pitch \
         -0.28000048 cell 0xA9B5000D; time 0.781; clock 306486365.4719249; weather auto",
    ),
    (
        "water-by-land",
        "dereth report 1; loc 0xA9B50011 [64.387108 22.052589 38.978706] 0.920786 0.000000 \
         0.000000 -0.390069; camera 0xA9B5 [60.244957 18.04098 42.136875] yaw -0.8014121 pitch \
         -0.27999994 cell 0xA9B50011; time 0.885; clock 306487158.54054123; weather auto",
    ),
    (
        "town-walls",
        "dereth report 1; loc 0xA9B40016 [64.420219 124.664795 66.005005] 0.893669 0.000000 \
         0.000000 -0.448728; camera 0xA9B4 [59.700523 120.963165 67.35501] yaw -0.9057002 pitch \
         0.02499907 cell 0xA9B40016; time 0.715; clock 306485861.59418947; weather auto",
    ),
];

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// One place drawn with `packs`: the frame, and what of the pack is held and how many frames
/// and seconds the pack's entries took to come.
fn draw(
    store: &Arc<dereth_dat::RetailDatStore>,
    report: &VisualReport,
    pack: &Arc<ArtPack>,
    folder: Option<std::path::PathBuf>,
    packs: RemasterPacks,
    water: RemasterWater,
    size: (u32, u32),
) -> (Vec<u8>, String) {
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let station = report_station(report, Some(3));
    let mut shot = Shot::open(store, &mut gpu, &station);
    shot.apply_report(report);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster_pack(Some(Arc::clone(pack)), folder);
    shot.scene.draw.set_remaster_packs(packs, &mut gpu);
    shot.scene.draw.set_remaster_water(water, &mut gpu);
    // The ground as the player draws it, which lays the pack's ground behind the splats.
    shot.scene.draw.set_remaster_terrain(
        dereth_scene::world_scene::RemasterTerrain::player(),
        &mut gpu,
    );
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    let started = std::time::Instant::now();
    shot.step_held(&mut gpu);
    let first = started.elapsed().as_secs_f64();
    // The player's ground is drawn once the world's vertices are read, off this thread.
    shot.scene.wait_for_remaster_ground();
    for _ in 0..5 {
        shot.step_held(&mut gpu);
    }
    let rgba = stations::capture(&mut gpu);
    let counts = shot.scene.draw.remaster_pack_counts(&mut gpu);
    let line = format!("first frame {first:.2} s; {counts:?}");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    (rgba, line)
}

/// Each place drawn by the remaster with nothing of the pack and with all of it: the counts
/// printed, and the pictures written under `DERETH_REMASTER_CAPTURE_DIR` when it is set.
#[test]
#[ignore = "instrument: draws an art pack at its places and writes pictures; run with --ignored"]
fn capture_an_art_pack_in_the_remaster() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let path =
        std::path::PathBuf::from(env("DERETH_ART_PACK").expect("DERETH_ART_PACK names the pack"));
    let started = std::time::Instant::now();
    let pack = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
    println!(
        "{}: {} models, {} materials, {} pictures, opened in {:.1} s; {} problems",
        pack.name,
        pack.models.len(),
        pack.materials.len(),
        pack.images.len(),
        started.elapsed().as_secs_f64(),
        pack.problems.len()
    );
    let folder = path.is_dir().then(|| path.clone());
    let size = super::remaster_frames::size();
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let wanted: Option<Vec<String>> = env("DERETH_REMASTER_PACK_SPOTS")
        .map(|v| v.split(',').map(|s| s.trim().to_owned()).collect());
    for (name, line) in SPOTS {
        if wanted
            .as_ref()
            .is_some_and(|w| !w.iter().any(|n| n == name))
        {
            continue;
        }
        let report = VisualReport::parse(line).expect("the place's report reads");
        let water = RemasterWater {
            on: true,
            ..RemasterWater::default()
        };
        for (way, packs, water) in [
            ("retail", RemasterPacks::default(), RemasterWater::default()),
            ("pack", RemasterPacks::all(), RemasterWater::default()),
            ("water", RemasterPacks::all(), water),
        ] {
            let (rgba, counts) = draw(&store, &report, &pack, folder.clone(), packs, water, size);
            println!("{name:16} {way:7} {counts}");
            if let Some(dir) = dir.as_ref() {
                let file = dir.join(format!("{name}-{way}.png"));
                stations::write_png(&file, size.0, size.1, &rgba);
            }
        }
    }
}

/// Walking up to the bushes with the pack drawn: the camera carried a quarter metre a frame
/// along its view, sixty frames; every tenth frame written under `DERETH_REMASTER_CAPTURE_DIR`.
#[test]
#[ignore = "instrument: walks up to an art pack's bushes and writes pictures; run with --ignored"]
fn walk_up_to_an_art_packs_bushes() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let path =
        std::path::PathBuf::from(env("DERETH_ART_PACK").expect("DERETH_ART_PACK names the pack"));
    let pack = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
    let folder = path.is_dir().then(|| path.clone());
    let size = super::remaster_frames::size();
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let (_, line) = SPOTS
        .iter()
        .find(|(n, _)| *n == "bushes-near")
        .expect("the bushes' place");
    let report = VisualReport::parse(line).expect("the place's report reads");
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let station = report_station(&report, Some(3));
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(Some(pack), folder);
    shot.scene
        .draw
        .set_remaster_packs(RemasterPacks::all(), &mut gpu);
    shot.scene.draw.set_remaster_terrain(
        dereth_scene::world_scene::RemasterTerrain::player(),
        &mut gpu,
    );
    shot.scene
        .draw
        .set_remaster(true, &store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.step_held(&mut gpu);
    // The player's ground is drawn once the world's vertices are read, off this thread.
    shot.scene.wait_for_remaster_ground();
    for _ in 0..5 {
        shot.step_held(&mut gpu);
    }
    for k in 0..60 {
        if let Some(c) = shot.held.as_mut() {
            let (s, co) = (
                dereth_primitives::num::math::sinf(c.yaw),
                dereth_primitives::num::math::cosf(c.yaw),
            );
            c.origin.x -= s * 0.25;
            c.origin.y += co * 0.25;
        }
        shot.step_held(&mut gpu);
        if let (Some(dir), true) = (dir.as_ref(), k % 10 == 9) {
            let now = stations::capture(&mut gpu);
            stations::write_png(&dir.join(format!("walk-{k:02}.png")), size.0, size.1, &now);
        }
    }
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
}

/// Spots from `PACK_SPOTS`, drawn by the remaster with all of the pack (`DERETH_ART_PACK`, or
/// none when it is `none`). A line is a free camera (`name block x y eye yaw pitch hour`) or a
/// body standing in an interior cell with the camera held behind it (`name cell <cell> x y z
/// heading-degrees hour`; `0 0 0` stands it where the cell has room). Either may end `with <setup>
/// <metres>`: an object of that setup made as the server makes one, standing that far ahead of the
/// camera, facing it.
#[test]
#[ignore = "local"]
fn capture_pack_spots() {
    use super::stations::{Clock, Place, Station};
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let named = env("DERETH_ART_PACK").expect("DERETH_ART_PACK");
    let stack: Vec<std::path::PathBuf> = named.split(':').map(std::path::PathBuf::from).collect();
    let path = stack[0].clone();
    let pack = (path.as_os_str() != "none").then(|| {
        Arc::new(if stack.len() > 1 {
            ArtPack::open_stack(&stack).expect("the stack opens")
        } else {
            ArtPack::open_path(&path).expect("the pack opens")
        })
    });
    if let Some(p) = &pack {
        println!(
            "PACK {} materials, problems {:?}",
            p.materials.len(),
            p.problems
        );
    }
    let folder = path.is_dir().then(|| path.clone());
    let size = super::remaster_frames::size();
    let dir = std::path::PathBuf::from(env("DERETH_REMASTER_CAPTURE_DIR").expect("dir"));
    // PACK_RAIN: 0 a sunny day, 1 a rainy day with the remaster's rain off, 2 with it on.
    let rain_mode: u32 = env("PACK_RAIN").and_then(|n| n.parse().ok()).unwrap_or(0);
    let (rain, rain_drawn) = (rain_mode > 0, rain_mode > 1);
    let spots = std::fs::read_to_string(env("PACK_SPOTS").expect("PACK_SPOTS")).expect("spots");
    for line in spots.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let name: &'static str = Box::leak(f[0].to_owned().into_boxed_str());
        let indoor = f[1] == "cell";
        let station = Station {
            name,
            why: "pack spots",
            place: if indoor {
                Place::Body {
                    cell: u32::from_str_radix(f[2].trim_start_matches("0x"), 16).expect("cell"),
                    origin: dereth_primitives::Vec3::new(
                        f[3].parse().expect("x"),
                        f[4].parse().expect("y"),
                        f[5].parse().expect("z"),
                    ),
                    yaw: f[6].parse().expect("yaw"),
                }
            } else {
                Place::Free {
                    block: u16::from_str_radix(f[1], 16).expect("block"),
                    x: f[2].parse().expect("x"),
                    y: f[3].parse().expect("y"),
                    eye: f[4].parse().expect("eye"),
                    yaw: f[5].parse().expect("yaw"),
                    pitch: f[6].parse().expect("pitch"),
                }
            },
            clock: if rain {
                Clock::Rainy(f.get(7).and_then(|h| h.parse().ok()).unwrap_or(0.45))
            } else {
                Clock::Sunny(f.get(7).and_then(|h| h.parse().ok()).unwrap_or(0.45))
            },
            weather: rain,
            land_radius: 3,
        };
        let cfg = DeviceConfig {
            width: size.0,
            height: size.1,
            remaster: true,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
        let mut shot = Shot::open(&store, &mut gpu, &station);
        let with = f.iter().position(|w| *w == "with").map(|i| {
            (
                u32::from_str_radix(f[i + 1].trim_start_matches("0x"), 16).expect("setup"),
                f[i + 2].parse::<f32>().expect("metres"),
            )
        });
        if indoor {
            // The camera held 1.2 m behind the body and 1.3 m up, at the spot's heading.
            let yaw: f32 = f[6].parse().expect("yaw");
            shot.follow = Some([1.2, 1.3, -0.1, yaw.to_radians()]);
        }
        shot.poll(&mut gpu);
        shot.scene
            .draw
            .set_remaster_pack(pack.clone(), folder.clone());
        shot.scene
            .draw
            .set_remaster_packs(RemasterPacks::all(), &mut gpu);
        shot.scene.draw.set_remaster_terrain(
            dereth_scene::world_scene::RemasterTerrain::player(),
            &mut gpu,
        );
        shot.scene
            .draw
            .set_remaster(true, &store, &mut gpu)
            .expect("the remaster starts");
        shot.scene
            .draw
            .set_remaster_light(RemasterLight::Pixel, &mut gpu);
        shot.scene.draw.set_remaster_weather(
            dereth_scene::world_scene::RemasterWeather {
                on: rain && rain_drawn,
            },
            &mut gpu,
        );
        shot.step_held(&mut gpu);
        if let Some((setup, ahead)) = with {
            shot.create_ahead(&station, setup, ahead);
        }
        if !indoor {
            shot.scene.wait_for_remaster_ground();
        }
        let settle: usize = env("PACK_SETTLE").and_then(|n| n.parse().ok()).unwrap_or(8);
        for _ in 0..settle {
            shot.step_held(&mut gpu);
        }
        let rgba = stations::capture(&mut gpu);
        let counts = shot.scene.draw.remaster_pack_counts(&mut gpu);
        println!("{name:16} {counts:?}");
        println!(
            "WEATHER {name} {:?}",
            shot.scene.draw.remaster_weather_stats(&mut gpu)
        );
        stations::write_png(&dir.join(format!("{name}.png")), size.0, size.1, &rgba);
        shot.scene
            .draw
            .set_remaster(false, &store, &mut gpu)
            .expect("the remaster stops");
    }
}

/// A walk from `PACK_WALK` (block x y eye yaw pitch hour metres frames, one line): the free camera
/// carried `metres` along its heading over `frames` frames, a thirtieth of a second each, drawn by
/// the remaster with all of the pack (`DERETH_ART_PACK`, or none when it is `none`), each frame
/// written as `walk-NNNN.png` under `DERETH_REMASTER_CAPTURE_DIR`.
#[test]
#[ignore = "local"]
fn walk_pack_path() {
    use super::stations::{Clock, Place, Station};
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let path = std::path::PathBuf::from(env("DERETH_ART_PACK").expect("DERETH_ART_PACK"));
    let pack = (path.as_os_str() != "none")
        .then(|| Arc::new(ArtPack::open_path(&path).expect("the pack opens")));
    let folder = path.is_dir().then(|| path.clone());
    let size = super::remaster_frames::size();
    let dir = std::path::PathBuf::from(env("DERETH_REMASTER_CAPTURE_DIR").expect("dir"));
    let line = std::fs::read_to_string(env("PACK_WALK").expect("PACK_WALK")).expect("walk");
    let f: Vec<&str> = line.split_whitespace().collect();
    let station = Station {
        name: "walk",
        why: "pack walk",
        place: Place::Free {
            block: u16::from_str_radix(f[0], 16).expect("block"),
            x: f[1].parse().expect("x"),
            y: f[2].parse().expect("y"),
            eye: f[3].parse().expect("eye"),
            yaw: f[4].parse().expect("yaw"),
            pitch: f[5].parse().expect("pitch"),
        },
        clock: Clock::Sunny(f[6].parse().expect("hour")),
        weather: false,
        land_radius: 3,
    };
    let metres: f32 = f[7].parse().expect("metres");
    let frames: usize = f[8].parse().expect("frames");
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(pack, folder);
    shot.scene
        .draw
        .set_remaster_packs(RemasterPacks::all(), &mut gpu);
    shot.scene.draw.set_remaster_terrain(
        dereth_scene::world_scene::RemasterTerrain::player(),
        &mut gpu,
    );
    shot.scene
        .draw
        .set_remaster(true, &store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..8 {
        shot.step_held(&mut gpu);
    }
    for k in 0..frames {
        #[allow(clippy::cast_precision_loss)] // a frame count
        shot.walk_to(&station, metres * k as f32 / frames.max(1) as f32);
        shot.step_by(&mut gpu, 1.0 / 30.0);
        let rgba = stations::capture(&mut gpu);
        stations::write_png(&dir.join(format!("walk-{k:04}.png")), size.0, size.1, &rgba);
    }
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
}
