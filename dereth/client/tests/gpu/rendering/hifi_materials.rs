//! An art pack's materials for retail pictures: Better Lighting draws a town's buildings with the
//! pack's material wherever the retail picture it stands for was, and the retail look draws the
//! retail pictures, pixel for pixel as Better Lighting draws them with no pack at all.
//!
//! The pack is made in the test: one flat magenta material for each of the pictures the houses
//! along a Holtburg street are drawn with, a color nothing in the town has.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware `wgpu` device off screen.
//!
//! Behaviour: none (experimental hi-fi art packs)

#![cfg(all(gpu, feature = "hifi"))]

use std::collections::HashMap;
use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_render::device::{AdapterKind, Backend, DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;

use crate::instruments::hifi_stations::{capture, Clock, Place, Shot, Station};

/// The frame's size.
const SIZE: (u32, u32) = (480, 270);

/// The pictures the houses along the street are drawn with: their stone, timbers, plaster,
/// boards and roofs.
const TOWN_PICTURES: [u32; 14] = [
    0x0500_2678,
    0x0500_26F0,
    0x0500_26F1,
    0x0500_26CC,
    0x0500_283C,
    0x0500_285E,
    0x0500_2862,
    0x0500_287E,
    0x0500_2894,
    0x0500_289A,
    0x0500_28F4,
    0x0500_28F5,
    0x0500_28F6,
    0x0500_28F7,
];

/// A pack of one flat magenta material for each of [`TOWN_PICTURES`].
fn magenta_pack() -> Arc<ArtPack> {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 4, 4);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("header");
        w.write_image_data(&[255, 0, 255, 255].repeat(16))
            .expect("data");
    }
    let mut manifest = String::from("[pack]\nname = \"Magenta\"\nformat = 1\n");
    for t in TOWN_PICTURES {
        manifest.push_str(&format!(
            "[[material]]\ntexture = \"{t:#010X}\"\nbase_color = \"m.png\"\nauthor = \"a\"\n\
             tool = \"t\"\nlicense = \"CC0-1.0\"\n"
        ));
    }
    let files: HashMap<&str, Vec<u8>> = HashMap::from([("m.png", png)]);
    Arc::new(
        ArtPack::open(&manifest.replace("0X", "0x"), &mut |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens"),
    )
}

/// A street of Holtburg by day, at eye height, looking north between two rows of houses.
fn street() -> Station {
    Station {
        name: "street",
        why: "a street of houses",
        place: Place::Free {
            block: 0xA9B4,
            x: 91.0,
            y: 112.0,
            eye: 1.8,
            yaw: 0.0,
            pitch: 0.0,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    }
}

/// The street drawn in Better Lighting with `boxes`, with `pack` when one is given, on a device of
/// its own: the frame, and how many draws the pack's materials drew.
fn drawn(pack: Option<&Arc<ArtPack>>, boxes: &str) -> (Vec<u8>, u64) {
    let store = crate::common::dats();
    let cfg = DeviceConfig {
        width: SIZE.0,
        height: SIZE.1,
        hifi: true,
        ..DeviceConfig::default()
    };
    let mut device = Gpu::new_on(Backend::Wgpu, None, &cfg)
        .unwrap_or_else(|e| panic!("this test needs a wgpu device and none opened: {e}"));
    assert_eq!(
        device.adapter_kind(),
        AdapterKind::Hardware,
        "{} is a software rasteriser",
        device.adapter_name()
    );
    let gpu = &mut device;
    let mut shot = Shot::open(&store, gpu, &street());
    if let Some(p) = pack {
        shot.scene
            .set_art_pack(&store, gpu, Some(Arc::clone(p)))
            .expect("the blocks bake for the pack");
    }
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(boxes).expect("the boxes parse");
    shot.poll(gpu);
    for _ in 0..12 {
        shot.step(gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the presentation's pipelines were not built in a minute"
        );
    }
    shot.step(gpu);
    assert!(gpu.hifi_failed().is_none(), "{:?}", gpu.hifi_failed());
    let report = gpu.hifi_report().expect("the presentation drew");
    assert!(report.composited, "the frame was drawn by the presentation");
    let materials = report
        .notes
        .iter()
        .find(|(name, _)| *name == "art pack material draws")
        .map_or(0, |(_, n)| *n);
    (capture(gpu), materials)
}

/// The share of the frame's pixels that are magenta: red and blue well over green.
fn magenta(rgba: &[u8]) -> f64 {
    let px = rgba.as_chunks::<4>().0;
    let n = px
        .iter()
        .filter(|p| {
            let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
            r - g > 60 && b - g > 60
        })
        .count();
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let share = n as f64 / px.len() as f64;
    share
}

const LIGHT: &str = "Lighting=1,NoAntiAliasing=1";

#[test]
fn better_lighting_draws_the_houses_with_the_packs_material_and_the_retail_look_with_retails() {
    let _gpu = crate::common::gpu_lock();
    let pack = magenta_pack();
    assert_eq!(
        pack.materials.len(),
        TOWN_PICTURES.len(),
        "{:?}",
        pack.problems
    );
    let (plain, none) = drawn(None, LIGHT);
    let (with_pack, drew) = drawn(Some(&pack), LIGHT);
    let (retail_look, kept) = drawn(Some(&pack), &format!("{LIGHT},RetailLook=1"));
    // The frames, in cargo's scratch folder, to look at when this fails.
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hifi-materials");
    std::fs::create_dir_all(&dir).expect("the scratch folder");
    for (name, rgba) in [
        ("plain", &plain),
        ("pack", &with_pack),
        ("retail-look", &retail_look),
    ] {
        crate::instruments::hifi_stations::write_png(
            &dir.join(format!("{name}.png")),
            SIZE.0,
            SIZE.1,
            rgba,
        );
    }
    assert_eq!(none, 0);
    assert!(drew > 0, "no draw took a material");
    assert_eq!(kept, 0, "the retail look drew a material");
    assert!(magenta(&plain) < 0.001, "{}", magenta(&plain));
    // The houses' walls, frames, boards and roofs take the material.
    assert!(
        magenta(&with_pack) > 0.05,
        "{} of the frame took the material",
        magenta(&with_pack)
    );
    // The retail look is Better Lighting with no pack, pixel for pixel.
    let differ = plain
        .iter()
        .zip(&retail_look)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differ, 0,
        "the retail look differs from no pack in {differ} bytes"
    );
}
