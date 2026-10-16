//! The light on an art pack's models through the day: a model standing for a retail tree whose
//! leaves give off light of their own keeps that light at dusk, so it follows Retail Look's tree
//! from noon into the evening.
//!
//! The pack is made in the test: a box of cut-out cards of flat green for Sho's huge tree, whose
//! retail leaves glow (a luminosity of 60 of 255 over most of them). The place is a Sho jungle
//! where that tree stands thick. Each way's own leaves are told apart by the census view; the
//! retail tree's are Retail Look's leaves less those the pack leaves to the world's own plants.
//! Drawn without the glow (the light's `pack_glow` knob at nought), the pack's tree falls far
//! further into the evening than retail's.
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

/// Sho's huge tree.
const HUGE_TREE: u32 = 0x0200_07D4;

/// The presentation's boxes the frames are drawn with.
const BOXES: &str =
    "Lighting=1,Shadows=1,GlobalIllumination=1,AmbientOcclusion=1,Sky=1,NoAntiAliasing=1";

/// A pack of one model for the huge tree: four cut-out cards of flat green standing round its
/// trunk as the sides of a box ten metres across, from four metres up to twenty-eight, each facing
/// out. Every leaf is on the crown's outside, so the crown's own shading leaves them as they are.
fn tree_pack() -> Arc<ArtPack> {
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, 1, 1);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("header");
        w.write_image_data(&[70, 130, 50, 255]).expect("data");
    }
    // glTF's height is its second axis. Each card: its four corners, its outward normal, the
    // picture's corners.
    let mut corners: Vec<([f32; 3], [f32; 3], [f32; 2])> = Vec::new();
    for (out, along) in [
        ([0.0f32, 0.0, 1.0], [1.0f32, 0.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ] {
        for (side, height, uv) in [
            (-5.0f32, 4.0f32, [0.0f32, 1.0f32]),
            (5.0, 4.0, [1.0, 1.0]),
            (5.0, 28.0, [1.0, 0.0]),
            (-5.0, 28.0, [0.0, 0.0]),
        ] {
            let p = [
                out[0] * 5.0 + along[0] * side,
                height,
                out[2] * 5.0 + along[2] * side,
            ];
            corners.push((p, [0.0, 1.0, 0.0], uv));
        }
    }
    let mut bin: Vec<u8> = Vec::new();
    for (p, _, _) in &corners {
        bin.extend(p.iter().flat_map(|v| v.to_le_bytes()));
    }
    for (_, n, _) in &corners {
        bin.extend(n.iter().flat_map(|v| v.to_le_bytes()));
    }
    for (_, _, t) in &corners {
        bin.extend(t.iter().flat_map(|v| v.to_le_bytes()));
    }
    for card in 0u16..4 {
        for i in [0u16, 1, 2, 0, 2, 3] {
            bin.extend((card * 4 + i).to_le_bytes());
        }
    }
    assert_eq!(bin.len(), 560);
    let gltf = r#"{"asset": {"version": "2.0"}, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2},
            "indices": 3, "material": 0}]}],
        "materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}},
            "alphaMode": "MASK", "alphaCutoff": 0.5, "doubleSided": true}],
        "textures": [{"source": 0}], "images": [{"uri": "leaf.png"}],
        "buffers": [{"uri": "cards.bin", "byteLength": 560}],
        "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 192},
            {"buffer": 0, "byteOffset": 192, "byteLength": 192},
            {"buffer": 0, "byteOffset": 384, "byteLength": 128},
            {"buffer": 0, "byteOffset": 512, "byteLength": 48}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 16, "type": "VEC3",
                "min": [-5.0, 4.0, -5.0], "max": [5.0, 28.0, 5.0]},
            {"bufferView": 1, "componentType": 5126, "count": 16, "type": "VEC3"},
            {"bufferView": 2, "componentType": 5126, "count": 16, "type": "VEC2"},
            {"bufferView": 3, "componentType": 5123, "count": 24, "type": "SCALAR"}]}"#;
    let manifest = format!(
        "[pack]\nname = \"Cards\"\nformat = 1\n[[model]]\nid = \"{HUGE_TREE:#010x}\"\n\
         foliage = true\nauthor = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n\
         [[model.lod]]\nfile = \"tree/cards.gltf\"\n"
    );
    let files: HashMap<&str, Vec<u8>> = HashMap::from([
        ("tree/cards.gltf", gltf.as_bytes().to_vec()),
        ("tree/cards.bin", bin),
        ("tree/leaf.png", png),
    ]);
    let pack = ArtPack::open(&manifest, &mut |path| {
        files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("{path}: missing"))
    })
    .expect("the pack opens");
    assert!(pack.problems.is_empty(), "{:?}", pack.problems);
    Arc::new(pack)
}

/// The Sho jungle at `clock`, two metres up, looking west-north-west through the huge trees.
fn jungle(clock: Clock) -> Station {
    Station {
        name: "jungle",
        why: "a Sho jungle where the huge tree stands thick",
        place: Place::Free {
            block: 0xF47B,
            x: 142.0,
            y: 130.0,
            eye: 2.0,
            yaw: 2.8798,
            pitch: 0.06,
        },
        clock,
        weather: false,
        land_radius: 2,
    }
}

/// `station` drawn in Better Lighting with the pack, its models (`retail_look` false) or the
/// retail look's, with `more` boxes and the light's knobs `knobs` (the tuning file's form), on a
/// device of its own.
fn drawn(
    station: &Station,
    pack: &Arc<ArtPack>,
    retail_look: bool,
    more: &str,
    knobs: &str,
) -> Vec<u8> {
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
    let mut shot = Shot::open_blending(&store, gpu, station, true);
    shot.scene
        .set_art_pack(&store, gpu, Some(Arc::clone(pack)))
        .expect("the blocks bake for the pack");
    let look = if retail_look { ",RetailLook=1" } else { "" };
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(&format!("{BOXES}{look}{more}"))
            .expect("the boxes parse");
    let (k, notes) = dereth_terrain::land::drawn::Knobs::parse(knobs);
    assert!(notes.is_empty(), "{notes:?}");
    shot.scene.set_terrain_knobs(k);
    shot.poll(gpu);
    // Every run takes the same steps, so they draw the same moment.
    shot.step(gpu);
    if !retail_look {
        shot.scene.wait_for_drawn_ground();
    }
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
    capture(gpu)
}

/// The census view's leaves: an art pack's (blue) and the world's own (cyan), each less a pixel's
/// width round its edge, as `(pack, world)` masks.
fn leaves(census: &[u8]) -> (Vec<bool>, Vec<bool>) {
    let px = census.as_chunks::<4>().0;
    let class = |f: &dyn Fn(i32, i32, i32) -> bool| -> Vec<bool> {
        px.iter()
            .map(|p| f(i32::from(p[0]), i32::from(p[1]), i32::from(p[2])))
            .collect()
    };
    let pack = class(&|r, g, b| b - g > 35 && g - r > 5 && b - r > 60);
    let world = class(&|r, g, b| g - r > 50 && b - r > 50 && (g - b).abs() < 40);
    (erode(&pack), erode(&world))
}

/// `mask` less every pixel next to one outside it.
fn erode(mask: &[bool]) -> Vec<bool> {
    let (w, h) = (SIZE.0 as usize, SIZE.1 as usize);
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            mask[i]
                && x > 0
                && y > 0
                && x + 1 < w
                && y + 1 < h
                && mask[i - 1]
                && mask[i + 1]
                && mask[i - w]
                && mask[i + w]
        })
        .collect()
}

/// The mean linear brightness of `frame` over `mask`, and how many pixels it holds.
fn brightness(frame: &[u8], mask: &[bool]) -> (f64, usize) {
    let lin = |c: u8| {
        let s = f32::from(c) / 255.0;
        f64::from(if s <= 0.04045 {
            s / 12.92
        } else {
            dereth_primitives::num::math::powf((s + 0.055) / 1.055, 2.4)
        })
    };
    let mut sum = 0.0;
    let mut n = 0usize;
    for (p, &m) in frame.as_chunks::<4>().0.iter().zip(mask) {
        if m {
            sum += 0.2126 * lin(p[0]) + 0.7152 * lin(p[1]) + 0.0722 * lin(p[2]);
            n += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let mean = sum / n.max(1) as f64;
    (mean, n)
}

/// How bright the pack's tree is against retail's at dusk, as a share of the same at noon, drawn
/// with the light's knobs `knobs`: 1 when the two fall into the evening together.
fn kept_into_the_evening(pack: &Arc<ArtPack>, knobs: &str) -> f64 {
    let noon = jungle(Clock::Sunny(0.5));
    let dusk = jungle(Clock::SunUp(5.0));
    // Each way's own leaves, from the census at noon.
    let (mine, others) = leaves(&drawn(&noon, pack, false, ",Debug=6", knobs));
    let (_, world) = leaves(&drawn(&noon, pack, true, ",Debug=6", knobs));
    let theirs: Vec<bool> = world.iter().zip(&others).map(|(w, o)| *w && !o).collect();
    let mut ratios = Vec::new();
    for (name, station) in [("noon", &noon), ("dusk", &dusk)] {
        let pack_frame = drawn(station, pack, false, "", knobs);
        let retail_frame = drawn(station, pack, true, "", knobs);
        let (p, pn) = brightness(&pack_frame, &mine);
        let (r, rn) = brightness(&retail_frame, &theirs);
        assert!(
            pn > 2000 && rn > 2000,
            "{name}: the trees cover {pn} and {rn} pixels"
        );
        eprintln!(
            "{knobs:?} {name}: the pack's tree {p:.4} over {pn} pixels, retail's {r:.4} over {rn}"
        );
        ratios.push(p / r);
    }
    eprintln!(
        "{knobs:?}: the pack's tree is {:.2} of retail's at noon and {:.2} at dusk",
        ratios[0], ratios[1]
    );
    ratios[1] / ratios[0]
}

#[test]
fn a_pack_tree_standing_for_a_retail_tree_that_glows_keeps_its_glow_from_noon_into_the_evening() {
    let _gpu = crate::common::gpu_lock();
    let pack = tree_pack();
    let kept = kept_into_the_evening(&pack, "");
    let unlit = kept_into_the_evening(&pack, "pack_glow = 0");
    // The cards are lit flatter than retail's tree, so they do not fall into the evening exactly
    // as it does (they keep about 0.7); without the glow they keep about a quarter.
    assert!(
        kept > 0.6,
        "the pack's tree keeps {kept:.2} of its noon brightness against retail's"
    );
    assert!(
        unlit < kept - 0.2,
        "without the glow it keeps {unlit:.2}, with it {kept:.2}"
    );
}
