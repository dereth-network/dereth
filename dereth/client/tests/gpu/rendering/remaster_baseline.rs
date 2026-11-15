//! Behaviour: none (tooling: the remaster's own frames kept to compare a later build with)
//!
//! The remaster's frames at the twelve stations of the light study, with every pass past retail's
//! own light that the player can tick, written by one build and compared by a later one: a change
//! that should leave the remaster's pictures alone (moving its code, sharing a pass) draws every
//! one byte for byte as before. Each station is drawn two ways on each device:
//!
//! - **Every pass:** the sun's cascades, the ambient occlusion indoors and out, both lamps, the
//!   atmosphere and the fog's volume, linear light with its bloom, the smoothing over the frames,
//!   the water and its puddles, and the player's ground.
//! - **Reach:** the same, drawn at two thirds of the display's size and brought up by FSR, the
//!   sun's shadows and the occlusion traced where the device traces rays.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device that draws the
//! remaster. `DERETH_REMASTER_SELF_DIR` names the folder the capture is written to and read from;
//! `DERETH_REMASTER_SELF_ONLY`, a comma list, keeps the comparison to the frames whose
//! `<device> <station>-<way>` contains one of its words (to look again at one that differed).

#![cfg(all(gpu, feature = "remaster"))]

use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{AdapterKind, Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::{
    RemasterLight, RemasterSettings, RemasterSky, RemasterTerrain, RemasterUpscaleMode,
};

use crate::instruments::remaster_frames::{self, STATIONS};
use crate::instruments::stations::{self, capture, Shot};

const W: u32 = 640;
const H: u32 = 480;

/// How a station is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    EveryPass,
    Reach,
}

impl Way {
    const ALL: [Self; 2] = [Self::EveryPass, Self::Reach];

    fn name(self) -> &'static str {
        match self {
            Self::EveryPass => "every-pass",
            Self::Reach => "reach",
        }
    }

    /// Its passes.
    fn settings(self) -> RemasterSettings {
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
            air: 0.0,
        };
        s.post.hdr = true;
        s.post.taa = true;
        s.water.on = true;
        s.water.puddles = true;
        s.terrain = RemasterTerrain::player();
        if self == Self::Reach {
            s.upscale.mode = RemasterUpscaleMode::Fsr;
            s.upscale.scale = 2.0 / 3.0;
            s.rt_shadows.on = true;
            s.rt_ao.on = true;
        }
        s
    }
}

/// The devices the remaster draws on in this build.
fn backends() -> Vec<Backend> {
    let mut v = vec![Backend::Vulkan];
    if cfg!(all(windows, feature = "d3d12")) {
        v.push(Backend::D3d12);
    }
    v
}

/// The frames a station is stepped before the one kept, as the light study steps them.
const STEPS: usize = 6;

/// The station drawn `way` on `backend`, on a device of its own, its light per pixel: its last
/// frame's pixels.
fn draw(
    store: &Arc<RetailDatStore>,
    station: &stations::Station,
    way: Way,
    backend: Backend,
) -> Vec<u8> {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(backend, None, &cfg).expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_settings(way.settings(), &mut gpu);
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    for k in 0..STEPS {
        shot.step(&mut gpu);
        // The world's vertices are read on a thread of their own, asked for by the first frame:
        // wait for them, so the frames after draw the ground from them.
        if k == 0 {
            shot.scene.wait_for_remaster_ground();
        }
    }
    let rgba = capture(&mut gpu);
    assert_eq!(gpu.size(), (W, H));
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(
        errors.is_empty(),
        "{} {way:?}: the validation layer: {errors:#?}",
        station.name
    );
    rgba
}

/// The digest the capture lists beside each picture: FNV-1a over its pixels, enough for a person
/// reading the list to tell two frames apart.
fn digest(rgba: &[u8]) -> u64 {
    rgba.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn folder() -> Option<std::path::PathBuf> {
    std::env::var_os("DERETH_REMASTER_SELF_DIR").map(std::path::PathBuf::from)
}

/// Whether the comparison looks at the frame `what` (`<device> <station>-<way>`): every one,
/// unless `DERETH_REMASTER_SELF_ONLY` names some.
fn compared(what: &str) -> bool {
    std::env::var("DERETH_REMASTER_SELF_ONLY").map_or(true, |only| {
        only.split(',')
            .map(str::trim)
            .any(|w| !w.is_empty() && what.contains(w))
    })
}

/// Not a claim: every station drawn every way on every device, written for
/// [`every_station_draws_what_the_base_build_drew`] to read in a later build:
/// `<device>/<station>-<way>.png` and `<device>/digest.txt`, under the folder
/// `DERETH_REMASTER_SELF_DIR` names.
#[test]
#[ignore = "instrument: writes a capture of this build; set DERETH_REMASTER_SELF_DIR and run with \
            --ignored"]
fn capture_the_remasters_frames_for_a_later_build() {
    let _gpu = crate::common::gpu_lock();
    let Some(root) = folder() else {
        return;
    };
    let store = crate::common::dats();
    let stations = remaster_frames::stations(&store, &STATIONS);
    assert_eq!(stations.len(), STATIONS.len(), "every station is found");
    for backend in backends() {
        let dir = root.join(backend.name());
        std::fs::create_dir_all(&dir).expect("the capture folder");
        let mut digests = String::new();
        for station in &stations {
            for way in Way::ALL {
                let rgba = draw(&store, station, way, backend);
                let name = format!("{}-{}", station.name, way.name());
                stations::write_png(&dir.join(format!("{name}.png")), W, H, &rgba);
                let _ = writeln!(digests, "{name} {:016x}", digest(&rgba));
            }
        }
        std::fs::write(dir.join("digest.txt"), digests).expect("digest.txt");
        eprintln!("wrote {}", dir.display());
    }
}

/// Every station, drawn every way on every device by this build, draws the base build's frame
/// byte for byte. The base build's frames are the capture
/// [`capture_the_remasters_frames_for_a_later_build`] writes, one folder per device.
#[test]
#[ignore = "compares with a capture of the base build: set DERETH_REMASTER_SELF_DIR and run with \
            --ignored"]
fn every_station_draws_what_the_base_build_drew() {
    let _gpu = crate::common::gpu_lock();
    let root = folder().expect("DERETH_REMASTER_SELF_DIR names the base build's capture");
    let store = crate::common::dats();
    let stations = remaster_frames::stations(&store, &STATIONS);
    assert_eq!(stations.len(), STATIONS.len(), "every station is found");
    let mut checked = 0;
    let mut differ = Vec::new();
    for backend in backends() {
        let dir = root.join(backend.name());
        for station in &stations {
            for way in Way::ALL {
                let name = format!("{}-{}", station.name, way.name());
                if !compared(&format!("{} {name}", backend.name())) {
                    continue;
                }
                let Some((width, height, base)) =
                    stations::read_png(&dir.join(format!("{name}.png")))
                else {
                    panic!("the base capture has no {} frame {name}", backend.name());
                };
                assert_eq!((width, height), (W, H), "{name}");
                let rgba = draw(&store, station, way, backend);
                let changed = stations::moved(&rgba, &base);
                eprintln!("{} {name}: {changed} bytes differ", backend.name());
                if changed > 0 {
                    differ.push(format!("{} {name}: {changed} bytes", backend.name()));
                    write_beside(&dir, &name, &rgba);
                }
                checked += 1;
            }
        }
    }
    assert!(
        differ.is_empty(),
        "frames differ from the base build's: {differ:#?}"
    );
    if std::env::var_os("DERETH_REMASTER_SELF_ONLY").is_none() {
        assert_eq!(checked, backends().len() * STATIONS.len() * Way::ALL.len());
    }
    assert!(checked > 0, "no frame was compared");
}

/// A frame that differs, written beside the base's as `<name>-now.png` for a person to look at.
fn write_beside(dir: &Path, name: &str, rgba: &[u8]) {
    stations::write_png(&dir.join(format!("{name}-now.png")), W, H, rgba);
}
