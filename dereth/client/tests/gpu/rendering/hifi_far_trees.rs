//! An art pack's trees drawn far off, past the blocks round the viewer, as their far pictures:
//! drawn out to their reach, and holding still under a still camera with the anti-aliasing on, a
//! frame counted after another, as the world without them does.
//!
//! The pack is made in the test: a flat ground of its own color for each dry terrain type (the
//! drawn ground, which the far trees stand on, is drawn only with a pack's ground), and the most
//! common tree's far pictures, a magenta crown of ragged leaves on a trunk, sixteen views round
//! it, so the far trees show as a color no ground has.
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

use crate::instruments::hifi_stations::{capture, write_png, Clock, Place, Shot, Station};

/// The frame's size.
const SIZE: (u32, u32) = (960, 540);
/// The most common tree, which the pack has far pictures for.
const TREE: &str = "0x02000258";
/// A tile of the far pictures, texels: eight views to a row, two rows.
const TILE: (u32, u32) = (32, 64);

/// A PNG of `w` by `h` texels.
fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("header");
        writer.write_image_data(rgba).expect("data");
    }
    out
}

/// A color for terrain type `t`, each its own, none of them magenta.
fn type_color(t: u8) -> [u8; 4] {
    let h = u32::from(t).wrapping_mul(0x9E37_79B9).rotate_left(7);
    let [a, b, c, _] = h.to_le_bytes();
    [40 + a / 2, 60 + b / 2, 30 + c / 3, 255]
}

/// The far pictures' two atlases: a magenta crown, a cone with ragged leaves cut out of it, on a
/// dark trunk; and its normal, out of the crown, with the depth on the plane through its axis.
/// Each view's crown is its own, narrow and wide in turn with its gaps elsewhere, and its leaves'
/// coverage runs from a third to all: what changes from view to view, and where a leaf's edge
/// is cut, would show from one frame to the next were either chosen by the frame's dither.
fn atlases() -> (Vec<u8>, Vec<u8>) {
    let (cols, rows) = (8u32, 2u32);
    let (w, h) = (TILE.0 * cols, TILE.1 * rows);
    let mut albedo = vec![0u8; (w * h * 4) as usize];
    let mut normal = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (tx, ty) = (x % TILE.0, y % TILE.1);
            // Across the tile from -1 to 1, and up it from 1 at the top to 0 at the foot.
            #[allow(clippy::cast_precision_loss)] // texel coordinates
            let (u, v) = (
                (tx as f32 + 0.5) / TILE.0 as f32 * 2.0 - 1.0,
                1.0 - (ty as f32 + 0.5) / TILE.1 as f32,
            );
            let k = ((y * w + x) * 4) as usize;
            let view = (y / TILE.1) * cols + x / TILE.0;
            let width = if view.is_multiple_of(2) { 1.15 } else { 0.7 };
            let trunk = v < 0.2 && u.abs() < 0.12;
            let crown = v >= 0.2 && u.abs() < (1.0 - v) * width;
            let hash = (x.wrapping_mul(73) ^ y.wrapping_mul(151) ^ view.wrapping_mul(97)) % 7;
            if trunk {
                albedo[k..k + 4].copy_from_slice(&[60, 40, 30, 255]);
            } else if crown && hash != 0 {
                // LINT-OK: a hash 1..6 to a coverage of a third to all.
                #[allow(clippy::cast_possible_truncation)]
                let cover = (85 + hash * 34).min(255) as u8;
                albedo[k..k + 4].copy_from_slice(&[200, 30, 190, cover]);
            } else {
                albedo[k..k + 4].copy_from_slice(&[200, 30, 190, 0]);
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: a direction's parts, -1..1, to 0..255.
            let n = [
                ((u * 0.5 + 0.5) * 255.0) as u8,
                128,
                ((v * 0.5 + 0.5) * 255.0) as u8,
                128,
            ];
            normal[k..k + 4].copy_from_slice(&n);
        }
    }
    (png(w, h, &albedo), png(w, h, &normal))
}

/// The far pictures' quad, sixteen meters across and twenty-four high, facing the eye; as
/// glTF, its height on the second axis.
fn quad() -> (String, Vec<u8>) {
    let (hw, lo, hi) = (8.0f32, -0.5f32, 23.5f32);
    let pos = [[-hw, lo, 0.0], [hw, lo, 0.0], [hw, hi, 0.0], [-hw, hi, 0.0]];
    let uv = [[0.0f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let mut bin = Vec::new();
    for p in pos {
        for c in p {
            bin.extend(c.to_le_bytes());
        }
    }
    for _ in 0..4 {
        for c in [0.0f32, 0.0, 1.0] {
            bin.extend(c.to_le_bytes());
        }
    }
    for t in uv {
        for c in t {
            bin.extend(c.to_le_bytes());
        }
    }
    for i in [0u32, 1, 2, 0, 2, 3] {
        bin.extend(i.to_le_bytes());
    }
    let gltf = format!(
        r#"{{"asset": {{"version": "2.0"}}, "scenes": [{{"nodes": [0]}}], "nodes": [{{"mesh": 0}}],
        "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2}},
            "indices": 3, "material": 0}}]}}],
        "materials": [{{"alphaMode": "MASK", "alphaCutoff": 0.5, "doubleSided": true,
            "pbrMetallicRoughness": {{"baseColorTexture": {{"index": 0}}, "metallicFactor": 0.0}},
            "normalTexture": {{"index": 1}}}}],
        "textures": [{{"source": 0}}, {{"source": 1}}],
        "images": [{{"uri": "far_albedo.png"}}, {{"uri": "far_normal.png"}}],
        "buffers": [{{"uri": "far.bin", "byteLength": {len}}}],
        "bufferViews": [{{"buffer": 0, "byteOffset": 0, "byteLength": 48}},
            {{"buffer": 0, "byteOffset": 48, "byteLength": 48}},
            {{"buffer": 0, "byteOffset": 96, "byteLength": 32}},
            {{"buffer": 0, "byteOffset": 128, "byteLength": 24}}],
        "accessors": [{{"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3",
                "min": [-{hw}, {lo}, 0.0], "max": [{hw}, {hi}, 0.0]}},
            {{"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3"}},
            {{"bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC2"}},
            {{"bufferView": 3, "componentType": 5125, "count": 6, "type": "SCALAR"}}]}}"#,
        len = bin.len()
    );
    (gltf, bin)
}

/// The pack: the grounds, and the tree's far pictures alone.
fn far_pack() -> Arc<ArtPack> {
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut manifest = String::from("[pack]\nname = \"Far trees\"\nformat = 1\n");
    let credit = "author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n";
    for t in (0..16).chain(21..32) {
        let name = format!("g{t}.png");
        files.insert(name.clone(), png(4, 4, &type_color(t).repeat(16)));
        manifest.push_str(&format!(
            "[[terrain]]\nground = {t}\nbase_color = \"{name}\"\nsize = 4\n{credit}"
        ));
    }
    let (gltf, bin) = quad();
    let (albedo, normal) = atlases();
    files.insert("tree/far.gltf".to_owned(), gltf.into_bytes());
    files.insert("tree/far.bin".to_owned(), bin);
    files.insert("tree/far_albedo.png".to_owned(), albedo);
    files.insert("tree/far_normal.png".to_owned(), normal);
    manifest.push_str(&format!(
        "[[model]]\nid = \"{TREE}\"\nfoliage = true\n{credit}[[model.lod]]\nfile = \"tree/far.gltf\"\nviews = 16\ncolumns = 8\n"
    ));
    Arc::new(
        ArtPack::open(&manifest, &mut |path| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens"),
    )
}

/// Over the thickest stand of the most common tree, a hundred and twenty meters up, looking out
/// across the forest round it: most of the land in the frame lies past the blocks round the
/// viewer, out to the horizon.
fn over_the_forest() -> Station {
    Station {
        name: "over-the-forest",
        why: "far trees to the horizon",
        place: Place::Free {
            block: 0x48C6,
            x: 96.0,
            y: 96.0,
            eye: 120.0,
            yaw: 0.6,
            pitch: -0.12,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    }
}

/// The boxes every frame is drawn with: the light, the sun's shadows and the sky, and the
/// anti-aliasing, whose jitter moves the picture a fraction of a pixel every frame.
const BOXES: &str = "Lighting=1,Shadows=1,Sky=1";

/// `station` opened on a device of its own with `pack`, the drawn ground's `knobs`, its ground
/// and far trees worked out and settled.
fn opened(station: &Station, pack: &Arc<ArtPack>, knobs: &str) -> (Gpu, Shot) {
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
    let mut shot = Shot::open(&store, gpu, station);
    shot.scene
        .set_art_pack(&store, gpu, Some(Arc::clone(pack)))
        .expect("the blocks bake for the pack");
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(BOXES).expect("the boxes parse");
    let (k, notes) = dereth_terrain::land::drawn::Knobs::parse(&knobs.replace(';', "\n"));
    assert!(notes.is_empty(), "{notes:?}");
    shot.scene.set_terrain_knobs(k);
    shot.poll(gpu);
    shot.step(gpu);
    shot.scene.wait_for_drawn_ground();
    for _ in 0..4 {
        shot.step(gpu);
    }
    shot.scene.wait_for_far_trees();
    shot.poll(gpu);
    for _ in 0..12 {
        shot.step(gpu);
        assert!(
            gpu.hifi_settle(std::time::Duration::from_secs(60)),
            "the presentation's pipelines were not built in a minute"
        );
    }
    assert!(gpu.hifi_failed().is_none(), "{:?}", gpu.hifi_failed());
    assert!(
        shot.scene.drawn_ground_active(),
        "the presentation draws the ground"
    );
    (device, shot)
}

/// Where the frames go, in cargo's scratch folder, to look at when a test fails.
fn keep(name: &str, rgba: &[u8]) {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("hifi-far-trees");
    std::fs::create_dir_all(&dir).expect("the scratch folder");
    write_png(&dir.join(format!("{name}.png")), SIZE.0, SIZE.1, rgba);
}

/// The pixels of the frame that are the far trees' magenta: red and blue well over green.
fn magenta(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .filter(|p| {
            let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
            r - g > 40 && b - g > 40
        })
        .count()
}

/// What a frame's notes say of `name`.
fn note(gpu: &Gpu, name: &str) -> u64 {
    gpu.hifi_report()
        .map(|r| r.notes)
        .unwrap_or_default()
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(0, |(_, v)| *v)
}

/// The most pixels that change by more than 8 levels from one frame to the next over `frames`
/// frames of a still camera with the clock held, after it has settled: of the whole frame, and of
/// the trees' pixels (magenta in either frame); and the first frame.
fn still_changes(shot: &mut Shot, gpu: &mut Gpu, frames: usize) -> (usize, usize, Vec<u8>) {
    // The anti-aliasing's history fills over its whole sequence of jitters, twice.
    for _ in 0..16 {
        shot.step_held(gpu);
    }
    let mut last = {
        shot.step_held(gpu);
        capture(gpu)
    };
    let first = last.clone();
    let (mut most, mut most_trees) = (0, 0);
    let tree = |p: &[u8; 4]| {
        let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
        r - g > 40 && b - g > 40
    };
    for _ in 0..frames {
        shot.step_held(gpu);
        let now = capture(gpu);
        let changed: Vec<bool> = last
            .as_chunks::<4>()
            .0
            .iter()
            .zip(now.as_chunks::<4>().0)
            .map(|(p, q)| (0..3).any(|c| p[c].abs_diff(q[c]) > 8) && (tree(p) || tree(q)))
            .collect();
        let all = last
            .as_chunks::<4>()
            .0
            .iter()
            .zip(now.as_chunks::<4>().0)
            .filter(|(p, q)| (0..3).any(|c| p[c].abs_diff(q[c]) > 8))
            .count();
        most = most.max(all);
        most_trees = most_trees.max(changed.iter().filter(|c| **c).count());
        last = now;
    }
    (most, most_trees, first)
}

#[test]
fn the_far_trees_are_drawn_out_to_their_reach_past_the_blocks_round_the_viewer() {
    let _gpu = crate::common::gpu_lock();
    let pack = far_pack();
    let (mut device, mut shot) = opened(&over_the_forest(), &pack, "tree_reach = 3000");
    let gpu = &mut device;
    shot.step_held(gpu);
    let with = capture(gpu);
    let standing = note(gpu, "art pack far trees");
    keep("reach-3000", &with);
    shot.scene
        .set_terrain_knobs(dereth_terrain::land::drawn::Knobs::parse("tree_reach = 1000").0);
    for _ in 0..4 {
        shot.step_held(gpu);
    }
    let shorter = capture(gpu);
    let fewer = note(gpu, "art pack far trees");
    keep("reach-1000", &shorter);
    shot.scene
        .set_terrain_knobs(dereth_terrain::land::drawn::Knobs::parse("far_trees = false").0);
    for _ in 0..4 {
        shot.step_held(gpu);
    }
    let none = capture(gpu);
    keep("far-trees-off", &none);
    eprintln!(
        "far trees standing: {standing} to 3 km, {fewer} to 1 km; magenta pixels {} to 3 km, {} to 1 km, {} with none",
        magenta(&with),
        magenta(&shorter),
        magenta(&none)
    );
    // Thousands of trees stand in the blocks out to three kilometers round the thickest forest.
    assert!(standing > 5000, "{standing} far trees to 3 km");
    assert!(
        fewer * 3 < standing,
        "{fewer} to 1 km against {standing} to 3 km"
    );
    // Drawn: the frame holds their color past the blocks round the viewer, less of it with a
    // shorter reach, and only the blocks' own trees' without them.
    assert!(
        magenta(&with) > magenta(&none) + 5000,
        "{} pixels with the far trees, {} without",
        magenta(&with),
        magenta(&none)
    );
    assert!(magenta(&shorter) < magenta(&with));
    assert!(magenta(&shorter) > magenta(&none));
}

#[test]
fn far_trees_hold_still_under_a_still_camera_frame_after_frame() {
    let _gpu = crate::common::gpu_lock();
    let pack = far_pack();
    // The trees thin out from close by, so most of them stand where the thinning takes them
    // away: each going there is drawn smaller, and holds still while the eye does.
    let (mut device, mut shot) = opened(&over_the_forest(), &pack, "far_trees = false");
    let gpu = &mut device;
    let (without, trees_without, plain) = still_changes(&mut shot, gpu, 12);
    keep("still-without", &plain);
    drop(shot);
    drop(device);
    let (mut device, mut shot) = opened(
        &over_the_forest(),
        &pack,
        "tree_thin_from = 200; tree_reach = 2500",
    );
    let gpu = &mut device;
    let (with, trees_with, trees) = still_changes(&mut shot, gpu, 12);
    keep("still-with", &trees);
    let shown = magenta(&trees).saturating_sub(magenta(&plain));
    eprintln!(
        "pixels changing by more than 8 levels a frame, at most: {without} without the far trees, {with} with them ({shown} far tree pixels); of the trees' pixels {trees_without} of {} without, {trees_with} of {} with",
        magenta(&plain),
        magenta(&trees)
    );
    assert!(
        shown > 5000,
        "{shown} far tree pixels: the far trees are drawn"
    );
    // The far trees add a few pixels of their edges' anti-aliasing at most: under one in a
    // hundred of those they cover, and nothing that changes the frame as a whole.
    assert!(
        with <= without + shown / 100,
        "{with} pixels changed a frame with the far trees, {without} without, over {shown} of theirs"
    );
    // And the trees' pictures, near and far, hold still but for their edges' anti-aliasing:
    // under 1.35 in a hundred of their pixels change a frame. A view chosen or a leaf cut by the
    // frame's dither changes more.
    let pictures = magenta(&trees);
    assert!(
        trees_with * 10_000 <= pictures * 135,
        "{trees_with} of the trees' {pictures} pixels changed a frame ({trees_without} of {} without the far trees)",
        magenta(&plain)
    );
}
