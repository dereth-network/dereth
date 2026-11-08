//! What an upscaler costs a remaster frame: the time from a frame's start until the device is
//! idle, split into what the processor spends and what is left to wait for.
//!
//! - `DERETH_REMASTER_UPSCALE`: `metalfx:0.5`, `fsr:0.5` or `off` (the default).
//! - `DERETH_REMASTER_STATIONS`: a comma list of stations (default `holtburg`).
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT`, the frame's own size (default 960x540).
//! - `DERETH_REMASTER_FRAMES`: the frames timed (default 240).
//! - `DERETH_REMASTER_MOVE`: set, the camera turns a little each frame.
//! - `DERETH_REMASTER_ALL`: set, every pass is on.
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the last frame's picture goes.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan device.
//!
//! Behaviour: none (instrument)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::time::Instant;

use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::world_scene::{
    RemasterAo, RemasterLight, RemasterPost, RemasterShadows, RemasterSky, RemasterUpscale,
    RemasterUpscaleMode,
};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{self, Shot};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// The upscaler a run asks for.
fn upscale(asked: &str) -> Option<RemasterUpscale> {
    let (mode, scale) = asked.split_once(':')?;
    Some(RemasterUpscale {
        mode: match mode {
            "metalfx" => RemasterUpscaleMode::MetalFx,
            "fsr" => RemasterUpscaleMode::Fsr,
            other => panic!("no upscaler {other}"),
        },
        scale: scale.parse().expect("a scale"),
        sharpness: 0.0,
        ..RemasterUpscale::default()
    })
}

/// Prints each station's frame time.
#[test]
#[ignore = "instrument: prints what a frame costs under the run's upscaler"]
fn print_the_frame_cost_under_the_upscaler() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let names = env("DERETH_REMASTER_STATIONS").unwrap_or_else(|| "holtburg".to_owned());
    let names: Vec<&str> = names.split(',').collect();
    let frames: usize = env("DERETH_REMASTER_FRAMES").map_or(240, |v| v.parse().expect("a count"));
    let turning = env("DERETH_REMASTER_MOVE").is_some();
    let mode = env("DERETH_REMASTER_UPSCALE").unwrap_or_else(|| "off".to_owned());
    let size = remaster_frames::size();
    for station in remaster_frames::stations(&store, &names) {
        let cfg = DeviceConfig {
            width: size.0,
            height: size.1,
            remaster: true,
            debug: false,
            ..DeviceConfig::default()
        };
        let mut gpu = Gpu::new_on(Backend::Vulkan, None, &cfg).expect("a Vulkan device");
        let mut shot = Shot::open(&store, &mut gpu, &station);
        shot.poll(&mut gpu);
        shot.scene
            .draw
            .set_remaster(true, &store, &mut gpu)
            .expect("the remaster starts");
        shot.scene
            .draw
            .set_remaster_light(RemasterLight::Pixel, &mut gpu);
        shot.scene.draw.set_remaster_post(
            RemasterPost {
                hdr: true,
                taa: true,
                ..RemasterPost::default()
            },
            &mut gpu,
        );
        shot.scene.draw.set_remaster_ao(
            RemasterAo {
                on: true,
                ..RemasterAo::default()
            },
            &mut gpu,
        );
        shot.scene.draw.set_remaster_shadows(
            RemasterShadows {
                on: true,
                ..RemasterShadows::default()
            },
            &mut gpu,
        );
        shot.scene.draw.set_remaster_sky(
            RemasterSky {
                atmosphere: true,
                fog: true,
                glow: 0.0,
                ..RemasterSky::default()
            },
            &mut gpu,
        );
        if env("DERETH_REMASTER_ALL").is_some() {
            let mut all = dereth_scene::world_scene::RemasterSettings::off();
            all.shadows.on = true;
            all.ao.on = true;
            all.lamps.indoor = true;
            all.lamps.outdoor = true;
            all.sky.atmosphere = true;
            all.sky.fog = true;
            all.post.hdr = true;
            all.post.taa = true;
            all.post.bloom = 0.04;
            all.lod.on = true;
            all.lod.impostors = true;
            all.terrain.splat = true;
            all.terrain.grid = true;
            all.terrain.drawn = true;
            all.terrain.horizon = true;
            all.water.on = true;
            all.water.puddles = true;
            all.water.rings = true;
            all.weather.on = true;
            // God Rays at the strength `DERETH_REMASTER_GOD_RAYS` names, 0 to 2: off by default.
            if let Some(level) = env("DERETH_REMASTER_GOD_RAYS").and_then(|v| v.trim().parse().ok())
            {
                all.sky = all.sky.with_god_rays(level);
            }
            shot.scene.draw.set_remaster_settings(all, &mut gpu);
        }
        if let Some(u) = upscale(&mode) {
            shot.scene.draw.set_remaster_upscale(u, &mut gpu);
        }
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_ground();
        for _ in 0..60 {
            shot.step_held(&mut gpu);
        }
        gpu.wait_idle().expect("idle");
        let (mut whole, mut processor) = (Vec::new(), Vec::new());
        for _ in 0..frames {
            if turning {
                shot.scene.camera.yaw += 0.002;
            }
            let t = Instant::now();
            shot.step_held(&mut gpu);
            processor.push(t.elapsed().as_secs_f64() * 1e3);
            gpu.wait_idle().expect("idle");
            whole.push(t.elapsed().as_secs_f64() * 1e3);
        }
        let waited: Vec<f64> = whole.iter().zip(&processor).map(|(w, p)| w - p).collect();
        let timings = shot.scene.draw.remaster_timings(&mut gpu);
        let scaler = timings
            .iter()
            .find(|t| t.0 == "metalfx")
            .map_or(0.0, |t| t.1);
        println!(
            "RESULT {} {}x{} upscale={mode}: whole {:.2} ms, processor {:.2} ms, waiting {:.2} ms, scaler {:.2} ms",
            station.name,
            size.0,
            size.1,
            median(whole),
            median(processor),
            median(waited),
            scaler,
        );
        println!("TIMINGS {} {mode}: {timings:?}", station.name);
        if let Some(dir) = env("DERETH_REMASTER_CAPTURE_DIR") {
            let rgba = stations::capture(&mut gpu);
            let (w, h) = gpu.size();
            stations::write_png(
                &std::path::Path::new(&dir).join(format!("{}-{mode}.png", station.name)),
                w,
                h,
                &rgba,
            );
        }
    }
}
