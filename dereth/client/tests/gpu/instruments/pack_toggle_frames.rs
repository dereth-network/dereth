//! What turning the art pack off and on again while the remaster draws leaves behind: at a place,
//! the remaster started with the pack's box ticked, the box unticked and ticked again through the
//! preferences poll as the player's option moves it, each state drawn until it settles; against
//! the same place started fresh with the box unticked, and ticked. For each state it prints what
//! the remaster chose and drew (the parts it chose levels for, the impostor cards, the far forest,
//! the G-buffer's and the shadows' draw items, the art pack's placements), the pixels of objects
//! in the sun and of shadowed ground, and how many pixels each toggled frame differs from the
//! fresh one by; with a still of each.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_ART_PACK`: the art packs drawn, joined by `;`, bottom first (required).
//! - `DERETH_TOGGLE_STATION`: the station drawn (default `vista`), or `DERETH_TOGGLE_PLACE`, a
//!   report line, over it.
//! - `DERETH_TOGGLE_RADII`: the scenery distances, a comma list (default `4,1`).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the stills go, `toggle-<radius>-<state>.png`, with
//!   where each toggled frame differs from its fresh one, `toggle-<radius>-<off|on>-differ.png`.
//! - `DERETH_REMASTER_LOOK_BOXES`, `DERETH_REMASTER_SIZE` as the look's instruments take them;
//!   the art pack's box and the scenery distance are set over them.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use super::remaster_look_frames::EVERY_BOX;
use super::stations::{self, capture, report_station, Shot, Station};

/// The frames drawn before a state is read: the pack's models in, the ground's offsets worked
/// out, the upscaler's history settled.
const SETTLE: usize = 48;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// One settled state of the remaster.
struct State {
    rgba: Vec<u8>,
    depth: Vec<f32>,
    line: String,
}

/// What the remaster in `shot` draws now, read after it settles.
fn read(shot: &mut Shot, gpu: &mut Gpu) -> State {
    for _ in 0..SETTLE {
        shot.step_held(gpu);
    }
    let rgba = capture(gpu);
    let draw = &shot.scene.draw;
    let lod = draw.remaster_lod_stats(gpu).unwrap_or_default();
    let imp = draw.remaster_impostor_stats(gpu).unwrap_or_default();
    let far = draw.remaster_forest_stats(gpu).unwrap_or_default();
    let counts = draw.remaster_counts(gpu).unwrap_or_default();
    let pack = draw.remaster_pack_counts(gpu).unwrap_or_default();
    let c = draw.remaster_capture(gpu).expect("the remaster drew");
    let sunlit = c.class.iter().filter(|k| **k == 2).count();
    let shaded = c.sun_visibility.as_ref().map_or(0, |v| {
        v.iter()
            .zip(&c.class)
            .filter(|(s, k)| **k == 1 && **s < 128)
            .count()
    });
    let line = format!(
        "scenery {} | lod parts {} fading {} turned {} | impostor cards {} | far standing {} | \
         items {} forward {} | shadow statics {} dynamics {} redrawn {} | pack placements {} \
         meshes {} | object pixels {sunlit} shaded ground {shaded}",
        draw.cfg.scenery_radius,
        lod.parts,
        lod.fading,
        lod.turned,
        imp.cards,
        far.standing,
        counts.items,
        counts.forward,
        counts.shadows.last_static_items,
        counts.shadows.last_dynamic_items,
        counts.shadows.last_redrawn,
        pack.placements,
        pack.meshes,
    );
    State {
        rgba,
        depth: c.depth,
        line,
    }
}

/// The pixels where `a` and `b` differ by more than a sixteenth in any channel, and the world's
/// pixels whose depth differs.
fn differ(a: &State, b: &State) -> (usize, usize) {
    let color = a
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.rgba.as_chunks::<4>().0)
        .filter(|(p, q)| (0..3).any(|k| p[k].abs_diff(q[k]) > 16))
        .count();
    let depth = a
        .depth
        .iter()
        .zip(&b.depth)
        .filter(|(x, y)| (**x - **y).abs() > 1e-5 * x.abs().max(y.abs()).max(1e-6))
        .count();
    (color, depth)
}

/// Where `a` and `b` differ as [`differ`] counts it, white on `a` darkened.
fn difference(a: &State, b: &State) -> Vec<u8> {
    a.rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.rgba.as_chunks::<4>().0)
        .flat_map(|(p, q)| {
            if (0..3).any(|k| p[k].abs_diff(q[k]) > 16) {
                [255, 255, 255, 255]
            } else {
                [p[0] / 4, p[1] / 4, p[2] / 4, 255]
            }
        })
        .collect()
}

/// `station` opened at `radius` on a device of `size`, `pack` given, with `boxes` asked and the
/// pack's box ticked as `ticked` says, first; then each of `then` asked in turn: each state read.
fn run(
    store: &Arc<RetailDatStore>,
    (station, report): (&Station, Option<&VisualReport>),
    pack: &Arc<ArtPack>,
    boxes: &str,
    (size, radius): ((u32, u32), u32),
    ticked: &[bool],
) -> Vec<State> {
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut shot = Shot::open_with(store, &mut gpu, station, false, Some(radius));
    if let Some(r) = report {
        shot.apply_report(r);
    }
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster_pack(Some(Arc::clone(pack)), None);
    let mut states = Vec::new();
    for (i, on) in ticked.iter().enumerate() {
        let asked = format!(
            "{boxes},RemasterScenery={radius},RemasterArtPack={}",
            u8::from(*on)
        );
        shot.scene.draw.cfg.render.fidelity =
            FidelityPreferences::parse_switch(&asked).expect("the boxes read");
        shot.poll(&mut gpu);
        // Each state as many frames from the remaster's start as a fresh one, so the upscaler's
        // jitter falls alike.
        shot.step_held(&mut gpu);
        if i == 0 {
            shot.scene.wait_for_remaster_ground();
        }
        states.push(read(&mut shot, &mut gpu));
    }
    drop(shot);
    drop(gpu);
    states
}

/// The art pack unticked and ticked again where the station stands, against fresh starts.
#[test]
#[ignore = "instrument: prints what an art pack toggle leaves behind and writes stills; run with --ignored"]
fn compare_an_art_pack_toggle_with_fresh_starts() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = env("DERETH_ART_PACK").map_or_else(
        || panic!("DERETH_ART_PACK names the packs"),
        |list| {
            let paths: Vec<std::path::PathBuf> =
                list.split(';').map(std::path::PathBuf::from).collect();
            Arc::new(ArtPack::open_stack(&paths).expect("the packs open"))
        },
    );
    let report = env("DERETH_TOGGLE_PLACE")
        .map(|l| VisualReport::parse(&l).expect("the place's report reads"));
    let station = report.as_ref().map_or_else(
        || {
            let name = env("DERETH_TOGGLE_STATION").unwrap_or_else(|| "vista".to_owned());
            super::remaster_frames::stations(&store, &[name.as_str()])
                .pop()
                .unwrap_or_else(|| panic!("the {name} station"))
        },
        |r| report_station(r, None),
    );
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let size = env("DERETH_REMASTER_SIZE").map_or((1280, 540), |_| super::remaster_frames::size());
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    let radii: Vec<u32> = env("DERETH_TOGGLE_RADII")
        .unwrap_or_else(|| "4,1".to_owned())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let place = (&station, report.as_ref());
    for radius in radii {
        let toggled = run(
            &store,
            place,
            &pack,
            &boxes,
            (size, radius),
            &[true, false, true],
        );
        let fresh_off = run(&store, place, &pack, &boxes, (size, radius), &[false]);
        let named = [
            ("on-fresh", &toggled[0]),
            ("off-toggled", &toggled[1]),
            ("on-toggled", &toggled[2]),
            ("off-fresh", &fresh_off[0]),
        ];
        for (name, s) in named {
            println!("radius {radius} {name}: {}", s.line);
            if let Some(dir) = dir.as_ref() {
                stations::write_png(
                    &dir.join(format!("toggle-{radius}-{name}.png")),
                    size.0,
                    size.1,
                    &s.rgba,
                );
            }
        }
        for (name, a, b) in [
            ("off", &toggled[1], &fresh_off[0]),
            ("on", &toggled[2], &toggled[0]),
        ] {
            let (c, d) = differ(a, b);
            println!(
                "radius {radius}: {name} toggled against fresh: {c} pixels differ, {d} depths"
            );
            if let Some(dir) = dir.as_ref() {
                stations::write_png(
                    &dir.join(format!("toggle-{radius}-{name}-differ.png")),
                    size.0,
                    size.1,
                    &difference(a, b),
                );
            }
        }
    }
}
