//! The remaster on the Direct3D 12 device: at the light study's stations it draws what it draws
//! on Vulkan, with every pass past retail's light off and with every pass on; and drawing, it
//! brings no shader compiler and no library of its own into the process.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware adapter with both Vulkan 1.3
//! and Direct3D 12 at shader model 6.6.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]
#![cfg(windows)]

use std::path::{Path, PathBuf};

use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::{RemasterLod, RemasterSettings, RemasterSky, RemasterTerrain};

use crate::instruments::remaster_frames::{self, run_set, Draw, Frame, Setup, STATIONS};
use crate::instruments::stations::Shot;

/// Every pass past retail's own light on, as far as the settings reach: the sun's shadows, the
/// occlusion indoors and out, the lamps, the sky's atmosphere and its fog, the scene in linear
/// light with its anti-aliasing, the water and its puddles, and the player's ground.
fn every_pass() -> RemasterSettings {
    let mut s = RemasterSettings::off();
    s.shadows.on = true;
    s.ao.on = true;
    s.ao.indoors = true;
    s.lamps.indoor = true;
    s.lamps.outdoor = true;
    s.sky = RemasterSky {
        atmosphere: true,
        fog: true,
        glow: 0.0,
        far_keep: 0.0,
    };
    s.post.hdr = true;
    s.post.taa = true;
    s.water.on = true;
    s.water.puddles = true;
    s.terrain = RemasterTerrain::player();
    s
}

/// How two of the remaster's frames differ.
#[derive(Debug, Clone, Copy)]
struct Apart {
    /// The pixels either drew the world on.
    world: usize,
    /// Their mean difference, display levels, over the three channels.
    mean: f64,
    /// The share of them whose largest channel differs by over 8, percent.
    over8: f64,
    /// The pixels only one of the two drew the world on, percent of the frame.
    silhouette: f64,
}

/// The remaster's frame `b` against its frame `a`, on the pixels either drew the world on.
fn apart(a: &Frame, b: &Frame) -> Apart {
    let n = a.rgba.len() / 4;
    let (mut world, mut sum, mut over, mut sil) = (0usize, 0u64, 0usize, 0usize);
    for i in 0..n {
        let drew_a = a.remaster_depth.get(i).is_some_and(|d| *d < 1.0);
        let drew_b = b.remaster_depth.get(i).is_some_and(|d| *d < 1.0);
        if drew_a != drew_b {
            sil += 1;
        }
        if !(drew_a || drew_b) {
            continue;
        }
        world += 1;
        let mut worst = 0;
        for c in 0..3 {
            let e = a.rgba[i * 4 + c].abs_diff(b.rgba[i * 4 + c]);
            sum += u64::from(e);
            worst = worst.max(e);
        }
        if worst > 8 {
            over += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    Apart {
        world,
        mean: if world == 0 {
            0.0
        } else {
            sum as f64 / (world * 3) as f64
        },
        over8: if world == 0 {
            0.0
        } else {
            over as f64 * 100.0 / world as f64
        },
        silhouette: sil as f64 * 100.0 / n.max(1) as f64,
    }
}

/// The most the remaster's frames on the two devices may be apart at a station, every pass off:
/// the mean in display levels, the share over 8 levels in percent, and the silhouettes in
/// percent of the frame.
const PASSES_OFF_APART: (f64, f64, f64) = (0.05, 0.1, 0.05);

/// The same with every pass on.
const EVERY_PASS_APART: (f64, f64, f64) = (0.5, 1.0, 0.1);

/// At every station, the remaster drawn on Direct3D 12 against itself drawn on Vulkan, retail's
/// light per pixel: with every pass past it off, and with every pass on, the frames are within
/// their bars of each other; on Direct3D 12 the remaster drew every frame and built no pipeline
/// after start-up.
#[test]
fn every_station_draws_on_direct3d_12_what_it_draws_on_vulkan() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let stations = remaster_frames::stations(&store, &STATIONS);
    assert_eq!(stations.len(), STATIONS.len());
    let size = remaster_frames::size();
    let mut worst = Vec::new();
    for (passes, settings, bars) in [
        ("passes off", RemasterSettings::off(), PASSES_OFF_APART),
        ("every pass", every_pass(), EVERY_PASS_APART),
    ] {
        for station in &stations {
            let on = |backend| {
                run_set(
                    &store,
                    station,
                    Draw::Pixel,
                    size,
                    &Setup {
                        settings: Some(settings),
                        backend: Some(backend),
                        ..Setup::default()
                    },
                )
            };
            let (vulkan, d3d12) = (on(Backend::Vulkan), on(Backend::D3d12));
            let a = apart(&vulkan, &d3d12);
            let what = format!("{} {passes}", station.name);
            eprintln!(
                "{what}: mean {:.3}, over 8 {:.2}%, silhouettes {:.3}%, {} world pixels",
                a.mean, a.over8, a.silhouette, a.world
            );
            assert!(a.world > 10_000, "{what}: world pixels compared");
            let counts = d3d12.counts.expect("the remaster counted");
            assert!(counts.frames >= 6, "{what}: the remaster drew every frame");
            assert_eq!(
                counts.pipelines_built, d3d12.pipelines_at_start,
                "{what}: pipelines built after start-up"
            );
            worst.push((what, a, bars));
        }
    }
    for (what, a, (mean, over8, silhouette)) in &worst {
        assert!(a.mean <= *mean, "{what}: mean {:.3}", a.mean);
        assert!(a.over8 <= *over8, "{what}: over 8 {:.2}%", a.over8);
        assert!(
            a.silhouette <= *silhouette,
            "{what}: silhouettes {:.3}%",
            a.silhouette
        );
    }
}

/// The share of the forest's pixels, percent, that may differ by over 8 levels between the two
/// devices with far trees drawn as cards: measured, 0.02.
const FAR_CARDS_APART: f64 = 0.05;

/// With far detail on, the forest at 1920 by 1080 draws on Direct3D 12 what it draws on Vulkan:
/// without its far trees as cards within the bar for every pass off; with them within
/// [`FAR_CARDS_APART`].
#[test]
fn far_detail_draws_on_direct3d_12_what_it_draws_on_vulkan() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let stations = remaster_frames::stations(&store, &["forest"]);
    let station = stations.first().expect("the forest");
    let mut worst = Vec::new();
    for (what, lod) in [
        ("cards", RemasterLod::ON),
        (
            "no cards",
            RemasterLod {
                impostors: false,
                ..RemasterLod::ON
            },
        ),
    ] {
        let on = |backend| {
            run_set(
                &store,
                station,
                Draw::Pixel,
                (1920, 1080),
                &Setup {
                    settings: Some(RemasterSettings::off()),
                    backend: Some(backend),
                    lod: Some(lod),
                    // The levels are worked out a share of the meshes a frame.
                    more_steps: 40,
                    ..Setup::default()
                },
            )
        };
        let (vulkan, d3d12) = (on(Backend::Vulkan), on(Backend::D3d12));
        let a = apart(&vulkan, &d3d12);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a count of pixels
        let over: usize = (a.over8 * a.world as f64 / 100.0) as usize;
        eprintln!(
            "forest {what}: mean {:.3}, over 8 {:.2}% ({over} of {} pixels), silhouettes {:.3}%",
            a.mean, a.over8, a.world, a.silhouette
        );
        worst.push((what, a.over8));
    }
    for (what, over8) in worst {
        let bar = if what == "cards" {
            FAR_CARDS_APART
        } else {
            PASSES_OFF_APART.1
        };
        assert!(over8 <= bar, "{what}: over 8 {over8:.2}%");
    }
}

/// Whether `path` lies in the Windows directory: the system's own libraries and its drivers'.
fn windows_own(path: &Path, windows: &Path) -> bool {
    let lower = |p: &Path| p.to_string_lossy().to_lowercase();
    lower(path).starts_with(&lower(windows))
}

/// A library's file name, lower case.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// On the Direct3D 12 device, the remaster started and drawing loads no library but the
/// system's own: no shader compiler (`dxcompiler.dll`, `dxil.dll`) anywhere in the process, no
/// Direct3D 12 runtime but the system's, and every library loaded after it started from the
/// Windows directory.
#[test]
fn the_remaster_on_direct3d_12_loads_no_compiler_and_no_library_of_its_own() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let windows = PathBuf::from(std::env::var_os("SystemRoot").expect("the Windows directory"));
    let station = remaster_frames::stations(&store, &["holtburg"])
        .pop()
        .expect("the town");
    let cfg = DeviceConfig {
        width: 640,
        height: 480,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(Backend::D3d12, None, &cfg).expect("a Direct3D 12 device");
    let mut shot = Shot::open(&store, &mut gpu, &station);
    shot.poll(&mut gpu);
    shot.step(&mut gpu);
    let before = dereth_gfx::d3d12::loaded_libraries();
    assert!(!before.is_empty(), "the process's libraries are listed");
    shot.scene
        .draw
        .set_remaster(true, &store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_settings(every_pass(), &mut gpu);
    for _ in 0..6 {
        shot.step(&mut gpu);
    }
    let frames = shot
        .scene
        .draw
        .remaster_counts(&mut gpu)
        .map_or(0, |c| c.frames);
    let after = dereth_gfx::d3d12::loaded_libraries();
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let new: Vec<&PathBuf> = after.iter().filter(|p| !before.contains(p)).collect();
    eprintln!("the remaster drew {frames} frames; libraries loaded since it started: {new:#?}");
    assert!(frames >= 6, "the remaster drew every frame");
    for p in &after {
        let name = file_name(p);
        assert!(
            name != "dxcompiler.dll" && name != "dxil.dll",
            "a shader compiler is loaded: {}",
            p.display()
        );
        if name == "d3d12core.dll" || name == "d3d12.dll" {
            assert!(
                windows_own(p, &windows),
                "a Direct3D 12 runtime not the system's: {}",
                p.display()
            );
        }
    }
    for p in &new {
        assert!(
            windows_own(p, &windows),
            "a library not the system's loaded once the remaster started: {}",
            p.display()
        );
    }
}
