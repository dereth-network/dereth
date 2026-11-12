//! Creatures standing where a server would put them, drawn by the remaster without an art pack and
//! with one, so a pack's bodies can be seen in the world beside what they replace.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! - `DERETH_ART_PACK`: the pack's folder or packed file (required).
//! - `DERETH_CREATURES_PLACE`: a report line saying where the camera stands (required).
//! - `DERETH_CREATURES_SETUP`, `DERETH_CREATURES_MTABLE`: the creatures' setup and motion table
//!   (hex, required).
//! - `DERETH_CREATURES_AT`: where they stand, a semicolon list of `block:x,y,heading` (block in
//!   hex, block-local meters, heading in degrees).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go, `retail.png` and `pack.png` (none are
//!   written without it).
//! - `DERETH_CREATURES_FRAMES`: frames stepped before each picture (default 90), so the creatures
//!   are seen at different moments of their idle.

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::report::VisualReport;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{RemasterLight, RemasterPacks};

use super::stations::{self, report_station, Shot};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn hex(v: &str) -> u32 {
    u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).expect("a hex id")
}

/// The place drawn with the creatures standing in it, the pack's bodies drawn or not.
fn draw(
    store: &Arc<dereth_dat::RetailDatStore>,
    report: &VisualReport,
    pack: &Arc<ArtPack>,
    folder: Option<std::path::PathBuf>,
    packs: RemasterPacks,
    size: (u32, u32),
) -> Vec<u8> {
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
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    let setup = hex(&env("DERETH_CREATURES_SETUP").expect("DERETH_CREATURES_SETUP"));
    let mtable = hex(&env("DERETH_CREATURES_MTABLE").expect("DERETH_CREATURES_MTABLE"));
    let at = env("DERETH_CREATURES_AT").unwrap_or_default();
    for (i, spot) in at.split(';').filter(|s| !s.trim().is_empty()).enumerate() {
        let (block, rest) = spot.split_once(':').expect("block:x,y,heading");
        let n: Vec<f32> = rest
            .split(',')
            .map(|v| v.trim().parse().expect("a number"))
            .collect();
        let id = 0x8300_0900 + u32::try_from(i).unwrap_or(0);
        #[allow(clippy::cast_possible_truncation)]
        let block = hex(block) as u16;
        shot.create_object(id, (setup, Some(mtable)), block, (n[0], n[1]), n[2]);
    }
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    let frames = env("DERETH_CREATURES_FRAMES")
        .and_then(|v| v.parse().ok())
        .unwrap_or(90);
    for _ in 0..frames {
        shot.step_held(&mut gpu);
    }
    let rgba = stations::capture(&mut gpu);
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    rgba
}

/// The creatures drawn without the pack and with it; the pictures written under
/// `DERETH_REMASTER_CAPTURE_DIR` when it is set.
#[test]
#[ignore = "instrument: draws creatures in the world with and without a pack; run with --ignored"]
fn draw_creatures_with_and_without_a_pack() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let path =
        std::path::PathBuf::from(env("DERETH_ART_PACK").expect("DERETH_ART_PACK names the pack"));
    let pack = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
    let folder = path.is_dir().then(|| path.clone());
    let report =
        VisualReport::parse(&env("DERETH_CREATURES_PLACE").expect("DERETH_CREATURES_PLACE"))
            .expect("the place's report reads");
    let size = super::remaster_frames::size();
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    for (way, packs) in [
        ("retail", RemasterPacks::default()),
        ("pack", RemasterPacks::all()),
    ] {
        let rgba = draw(&store, &report, &pack, folder.clone(), packs, size);
        if let Some(dir) = dir.as_ref() {
            stations::write_png(&dir.join(format!("{way}.png")), size.0, size.1, &rgba);
        }
        println!("{way}: drawn");
    }
}
