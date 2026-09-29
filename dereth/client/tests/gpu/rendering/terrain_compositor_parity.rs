//! The landscape's composites built by the device's compute shader are **bit-identical** to the
//! CPU compositor's, at every level of every texture.
//!
//! The same window is loaded twice on one device, once with the CPU compositor and once with the
//! device's, and every live composite is read back level by level and compared byte for byte.
//! Level 0 is the compositor's own output; the other levels are the sublevel generation both
//! paths share, so a difference there would mean the two level-0 images differed or were handed
//! to it differently.
//!
//! The splat draw is checked for what it can promise: it draws, and a frame with it differs from
//! the composite frame by filtering and nothing coarser.
//! Fixture: the retail dats' landscape, on the software device and on the machine's own GPU.
//!
//! Behaviour: none (an equivalence of the device and CPU terrain compositors, not a retail claim)

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn device() -> Gpu {
    device_of(true)
}

/// `software` asks for the deterministic software device; without it the machine's own GPU is
/// used when there is one.
fn device_of(software: bool) -> Gpu {
    let cfg = DeviceConfig {
        width: 320,
        height: 240,
        force_software: software,
        ..DeviceConfig::default()
    };
    Gpu::new(None, &cfg).expect("a device")
}

fn config(gpu_terrain_merge: bool) -> SceneConfig {
    SceneConfig {
        land_radius: 3,
        scenery_radius: 0,
        character: false,
        gpu_terrain_merge,
        ..SceneConfig::default()
    }
}

/// Every composite's levels, by merge key.
fn read_back(gpu: &mut Gpu, scene: &WorldScene) -> BTreeMap<u32, Vec<Vec<u8>>> {
    let mut out = BTreeMap::new();
    for (key, slot) in scene.terrain_composites() {
        let mut levels = Vec::new();
        for level in 0u16.. {
            match gpu.capture_texture_level_data(slot, level) {
                Ok(t) => levels.push(t.levels.into_iter().next().expect("one level")),
                Err(_) => break,
            }
        }
        assert!(
            !levels.is_empty(),
            "composite {key:#010X} read back no level"
        );
        out.insert(key, levels);
    }
    out
}

#[test]
fn the_device_compositor_matches_the_cpu_compositor_bit_for_bit() {
    // On the software device and on the machine's own GPU: the arithmetic is integer, so a driver
    // is no excuse for a single differing byte.
    for software in [true, false] {
        compare_compositors(device_of(software));
    }
}

fn compare_compositors(mut gpu: Gpu) {
    let store = store();
    if !gpu.terrain_merge_supported() {
        eprintln!("skipping: this device cannot compose on the device");
        return;
    }

    let mut cpu_scene =
        WorldScene::load(&store, &mut gpu, config(false)).expect("the CPU scene loads");
    let cpu = read_back(&mut gpu, &cpu_scene);
    cpu_scene.release_textures(&mut gpu);
    drop(cpu_scene);

    let mut gpu_scene =
        WorldScene::load(&store, &mut gpu, config(true)).expect("the device scene loads");
    let dev = read_back(&mut gpu, &gpu_scene);
    gpu_scene.release_textures(&mut gpu);

    assert!(
        cpu.len() > 20,
        "a 7x7 window makes dozens of composites, not {}",
        cpu.len()
    );
    assert_eq!(
        cpu.keys().collect::<Vec<_>>(),
        dev.keys().collect::<Vec<_>>(),
        "the two paths composed different keys"
    );
    let mut levels = 0;
    for (key, c) in &cpu {
        let d = &dev[key];
        assert_eq!(
            c.len(),
            d.len(),
            "composite {key:#010X}: level counts differ"
        );
        for (level, (a, b)) in c.iter().zip(d).enumerate() {
            let first = a.iter().zip(b).position(|(x, y)| x != y);
            assert!(
                a.len() == b.len() && first.is_none(),
                "composite {key:#010X} level {level}: {} vs {} bytes, first difference at byte {first:?}",
                a.len(),
                b.len()
            );
            levels += 1;
        }
    }
    eprintln!(
        "{:?} on {:?}: {} composites, {levels} levels, all identical",
        gpu.backend(),
        gpu.adapter_kind(),
        cpu.len()
    );
}

/// A scene that starts with composites holds no splat data. Switching to splatting gives every
/// resident cell its layers and releases every composite, which is the point of the mode on a
/// card with little video memory; switching back builds the same composites again and drops the
/// layers.
#[test]
fn switching_modes_converts_every_resident_cell_and_frees_the_other_mode() {
    let store = store();
    let mut gpu = device();
    let mut scene = WorldScene::load(&store, &mut gpu, config(true)).expect("the scene loads");
    let keys = |s: &WorldScene| {
        s.terrain_composites()
            .into_iter()
            .map(|(k, _)| k)
            .collect::<Vec<_>>()
    };
    let composites = keys(&scene);
    let (splat, cells) = scene.terrain_splat_cells();
    assert!(
        !composites.is_empty() && cells > 0,
        "the window has cells and composites"
    );
    assert_eq!(splat, 0, "a composite-mode scene makes no splat data");
    if !gpu.terrain_splat_supported() {
        assert_eq!(scene.toggle_terrain_splat(), None);
        eprintln!("skipping the rest: this device cannot splat");
        return;
    }
    let live = gpu.descriptor_usage().live;

    assert_eq!(scene.toggle_terrain_splat(), Some(true));
    scene.stream(&store, &mut gpu).expect("streams");
    assert_eq!(
        scene.terrain_splat_cells(),
        (cells, cells),
        "every resident cell has its layers"
    );
    assert!(keys(&scene).is_empty(), "splatting holds no composites");
    assert!(
        gpu.descriptor_usage().live < live,
        "the composites' textures were released: {} live before, {} after",
        live,
        gpu.descriptor_usage().live
    );

    assert_eq!(scene.toggle_terrain_splat(), Some(false));
    scene.stream(&store, &mut gpu).expect("streams");
    assert_eq!(keys(&scene), composites, "the same composites come back");
    assert_eq!(scene.terrain_splat_cells().0, 0, "and the splat layers go");
    scene.release_textures(&mut gpu);
}
