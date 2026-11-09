//! The art pack as the remaster draws it: its models in place of the objects it has them for, by
//! kind as their switches say, every one of its assets held; a cut-out picture's edge free of
//! the color its cut leaves out; its lighting laid over retail's light while any of it is drawn;
//! and with every switch off, the frame drawn without it.
//!
//! The pack is made in the test: the most common tree's model, an upright card twelve metres
//! high carrying a green leaf whose cut-away texels hold a magenta key; or a flat magenta
//! material for each of the pictures the houses along a Holtburg street are drawn with.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{
    RemasterCapture, RemasterLight, RemasterPackCounts, RemasterPacks, RemasterPost,
};

use crate::instruments::remaster_frames;
use crate::instruments::stations::{Clock, Place, Shot, Station};

const W: u32 = 640;
const H: u32 = 480;

/// The most common tree, which the pack has a model for.
const TREE: &str = "0x02000258";

fn device() -> Gpu {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    gpu
}

fn station(store: &RetailDatStore, name: &str) -> Station {
    remaster_frames::stations(store, &[name])
        .pop()
        .unwrap_or_else(|| panic!("the {name} station"))
}

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

/// The leaf: a green disk, its texels kept, in a magenta key the cut leaves out; few texels, so
/// a card near the eye is drawn from the picture magnified, each texel blended with the next.
fn leaf() -> Vec<u8> {
    let side = 16u32;
    let mut rgba = Vec::new();
    for y in 0..side {
        for x in 0..side {
            #[allow(clippy::cast_precision_loss)] // a small picture's texels
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            rgba.extend(if dx * dx + dy * dy < 6.5 * 6.5 {
                [40, 200, 60, 255]
            } else {
                [255, 0, 255, 0]
            });
        }
    }
    png(side, side, &rgba)
}

/// An upright card `width` across and `height` high, as glTF (its height on the second axis),
/// cut at half its picture's alpha and drawn from both sides.
fn card(width: f32, height: f32) -> (String, Vec<u8>) {
    let hw = width / 2.0;
    let pos = [
        [-hw, 0.0, 0.0],
        [hw, 0.0, 0.0],
        [hw, height, 0.0],
        [-hw, height, 0.0],
    ];
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
            "pbrMetallicRoughness": {{"baseColorTexture": {{"index": 0}}, "metallicFactor": 0.0}}}}],
        "textures": [{{"source": 0}}],
        "images": [{{"uri": "leaf.png"}}],
        "buffers": [{{"uri": "card.bin", "byteLength": {len}}}],
        "bufferViews": [{{"buffer": 0, "byteOffset": 0, "byteLength": 48}},
            {{"buffer": 0, "byteOffset": 48, "byteLength": 48}},
            {{"buffer": 0, "byteOffset": 96, "byteLength": 32}},
            {{"buffer": 0, "byteOffset": 128, "byteLength": 24}}],
        "accessors": [{{"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3",
                "min": [-{hw}, 0.0, 0.0], "max": [{hw}, {height}, 0.0]}},
            {{"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3"}},
            {{"bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC2"}},
            {{"bufferView": 3, "componentType": 5125, "count": 6, "type": "SCALAR"}}]}}"#,
        len = bin.len()
    );
    (gltf, bin)
}

/// The pack's files: the tree's model, a card of two levels (the nearer one larger), and its
/// `lighting` section, when it has one.
fn files(lighting: Option<&str>) -> HashMap<String, Vec<u8>> {
    let mut files = HashMap::new();
    let credit = "author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n";
    let mut manifest = String::new();
    if let Some(l) = lighting {
        manifest.push_str(&format!("[lighting]\n{l}\n"));
    }
    manifest.push_str("[pack]\nname = \"Cards\"\nformat = 1\n");
    manifest.push_str(&format!(
        "[[model]]\nid = \"{TREE}\"\nfoliage = true\ncategory = \"tree\"\n{credit}\
         [[model.lod]]\nfile = \"near/card.gltf\"\ndistance = 60.0\n\
         [[model.lod]]\nfile = \"far/card.gltf\"\n"
    ));
    files.insert("manifest.toml".to_owned(), manifest.into_bytes());
    for (level, (w, h)) in [("near", (8.0, 12.0)), ("far", (6.0, 10.0))] {
        let (gltf, bin) = card(w, h);
        files.insert(format!("{level}/card.gltf"), gltf.into_bytes());
        files.insert(format!("{level}/card.bin"), bin);
        files.insert(format!("{level}/leaf.png"), leaf());
    }
    files
}

/// The pack of `files`, opened from memory.
fn in_memory(files: &HashMap<String, Vec<u8>>) -> Arc<ArtPack> {
    let manifest = String::from_utf8(files["manifest.toml"].clone()).expect("text");
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

/// The pack of `files`, written to a folder of its own and opened from it.
fn in_a_folder(files: &HashMap<String, Vec<u8>>, name: &str) -> (Arc<ArtPack>, PathBuf) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("remaster-packs")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    for (path, bytes) in files {
        let at = dir.join(path);
        std::fs::create_dir_all(at.parent().expect("a folder")).expect("the folder");
        std::fs::write(&at, bytes).expect("written");
    }
    (
        Arc::new(ArtPack::open_dir(&dir).expect("the pack opens")),
        dir,
    )
}

/// One frame of the remaster, and what of the pack it holds.
struct Frame {
    capture: RemasterCapture,
    counts: RemasterPackCounts,
}

/// The remaster at `station`, its light per pixel, with `pack` (from its folder, when it has
/// one) drawn as `packs` says; stepped `frames` frames held still; the last.
fn run(
    store: &Arc<RetailDatStore>,
    station: &Station,
    pack: Option<(Arc<ArtPack>, Option<PathBuf>)>,
    packs: RemasterPacks,
    frames: usize,
) -> Frame {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    let (pack, folder) = pack.map_or((None, None), |(p, f)| (Some(p), f));
    shot.scene.draw.set_remaster_pack(pack, folder);
    shot.scene.draw.set_remaster_packs(packs, &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    for _ in 0..frames {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    let counts = shot
        .scene
        .draw
        .remaster_pack_counts(&mut gpu)
        .expect("the remaster is on");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    Frame { capture, counts }
}

fn trees() -> RemasterPacks {
    RemasterPacks {
        trees: true,
        ..RemasterPacks::default()
    }
}

/// The pixels of the world drawn (the G-buffer's), green as the leaf is.
fn leafy(c: &RemasterCapture) -> usize {
    (0..c.class.len())
        .filter(|i| c.class[*i] != 0)
        .filter(|i| {
            let p = &c.color[i * 4..i * 4 + 3];
            u16::from(p[1]) > u16::from(p[0]) * 2 && u16::from(p[1]) > u16::from(p[2]) * 2
        })
        .count()
}

/// The pixels of the world drawn, tinted by the magenta key: red and blue both over green, and
/// together well over twice it.
fn keyed(c: &RemasterCapture) -> usize {
    (0..c.class.len())
        .filter(|i| c.class[*i] != 0)
        .filter(|i| {
            let [r, g, b] = [0, 1, 2].map(|k| u16::from(c.color[i * 4 + k]));
            r > g && b > g && r + b > 2 * g + 20
        })
        .count()
}

/// **The pack's trees take the place of the trees it has a model for, every asset of theirs
/// held**: in the forest, with the trees' switch on, the trees are drawn as the pack's cards
/// (leaf-green pixels where there were few), each model's two levels, their surfaces and their
/// pictures all held. With every switch off, nothing is swapped and the frame is the one drawn
/// with no pack at all; with only the plants' switch on, the trees (which the pack calls trees)
/// stay retail's.
#[test]
fn the_packs_trees_take_the_place_of_the_trees_it_has_a_model_for() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let pack = in_memory(&files(None));
    let none = run(&store, &st, None, RemasterPacks::default(), 6);
    let on = run(&store, &st, Some((Arc::clone(&pack), None)), trees(), 6);
    let off = run(
        &store,
        &st,
        Some((Arc::clone(&pack), None)),
        RemasterPacks::default(),
        6,
    );
    let plants = RemasterPacks {
        plants: true,
        ..RemasterPacks::default()
    };
    let wrong_kind = run(&store, &st, Some((pack, None)), plants, 6);
    let c = on.counts;
    eprintln!(
        "forest: {c:?}; leaf-green pixels {} with the pack's trees, {} without",
        leafy(&on.capture),
        leafy(&none.capture)
    );
    assert!(c.placements > 20, "{c:?}");
    assert_eq!((c.meshes, c.meshes_held), (2, 2), "{c:?}");
    assert_eq!(c.surfaces, c.surfaces_held, "{c:?}");
    assert_eq!(c.pictures, c.pictures_held, "{c:?}");
    assert!(c.surfaces >= 2 && c.pictures >= 2, "{c:?}");
    assert!(
        leafy(&on.capture) > leafy(&none.capture) + 2_000,
        "the cards are drawn"
    );
    assert_eq!(off.counts.placements, 0);
    assert_eq!(
        off.capture.color, none.capture.color,
        "off, the frame without a pack"
    );
    assert_eq!(wrong_kind.counts.placements, 0, "the trees are not plants");
}

/// **A cut-out picture's edge carries none of the color its cut leaves out**: the leaf cards
/// drawn from the pack's folder, whose cut-out pictures the remaster makes again with their
/// cut-away texels filled from the edge, show no magenta-tinted pixel more than the forest
/// without the pack does; the same pack opened from memory, its pictures as the pack made them,
/// shows a fringe of them. The leaves are drawn with no pull toward the look's foliage hue, so the
/// key's color is seen as it is.
#[test]
fn a_cut_out_pictures_edge_carries_none_of_the_color_its_cut_leaves_out() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let f = files(Some("foliage_pull = 0.0"));
    let none = run(&store, &st, None, RemasterPacks::default(), 6);
    let made = run(&store, &st, Some((in_memory(&f), None)), trees(), 6);
    let (pack, dir) = in_a_folder(&f, "leaf");
    let filled = run(&store, &st, Some((pack, Some(dir))), trees(), 6);
    eprintln!(
        "forest: key-tinted pixels {} filled, {} as made, {} with no pack",
        keyed(&filled.capture),
        keyed(&made.capture),
        keyed(&none.capture)
    );
    assert!(
        keyed(&made.capture) > keyed(&none.capture) + 100,
        "a fringe as made"
    );
    assert!(
        keyed(&filled.capture) <= keyed(&none.capture) + 20,
        "no fringe once filled"
    );
}

/// **The pack's lighting is laid over retail's light while any of it is drawn**: a pack whose
/// `[lighting]` takes the sun away draws the forest darker than the same pack without the
/// section, with its trees on; with every switch off it changes nothing.
#[test]
fn the_packs_lighting_is_laid_over_retails_light_while_any_of_it_is_drawn() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let dark = in_memory(&files(Some("sun = 0.0")));
    let plain = in_memory(&files(None));
    let brightness = |c: &RemasterCapture| -> u64 {
        (0..c.class.len())
            .filter(|i| c.class[*i] != 0)
            .map(|i| u64::from(c.color[i * 4]) + u64::from(c.color[i * 4 + 1]))
            .sum()
    };
    let lit = run(&store, &st, Some((Arc::clone(&dark), None)), trees(), 6);
    let unlit = run(&store, &st, Some((plain, None)), trees(), 6);
    let off = run(&store, &st, Some((dark, None)), RemasterPacks::default(), 6);
    let none = run(&store, &st, None, RemasterPacks::default(), 6);
    eprintln!(
        "forest: brightness {} with the pack's lighting, {} without it",
        brightness(&lit.capture),
        brightness(&unlit.capture)
    );
    assert!(brightness(&lit.capture) * 10 < brightness(&unlit.capture) * 9);
    assert_eq!(off.capture.color, none.capture.color);
}

/// The pictures the houses along a Holtburg street are drawn with: their stone, timbers, plaster,
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
    magenta_pack_of(None)
}

/// [`magenta_pack`], each material's roughness picture all `roughness` where it says.
fn magenta_pack_of(roughness: Option<u8>) -> Arc<ArtPack> {
    let mut manifest = String::from("[pack]\nname = \"Magenta\"\nformat = 1\n");
    let rough_line = if roughness.is_some() {
        "roughness = \"r.png\"\n"
    } else {
        ""
    };
    for t in TOWN_PICTURES {
        manifest.push_str(&format!(
            "[[material]]\ntexture = \"0x{t:08X}\"\nbase_color = \"m.png\"\n{rough_line}\
             author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n"
        ));
    }
    let picture = png(4, 4, &[255, 0, 255, 255].repeat(16));
    let rough = png(4, 4, &[roughness.unwrap_or(0), 0, 0, 255].repeat(16));
    Arc::new(
        ArtPack::open(&manifest, &mut |path| match path {
            "m.png" => Ok(picture.clone()),
            "r.png" => Ok(rough.clone()),
            _ => Err(format!("{path}: missing")),
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

/// The pixels of the world drawn, magenta: red and blue each over twice green.
fn magenta(c: &RemasterCapture) -> usize {
    (0..c.class.len())
        .filter(|i| c.class[*i] != 0)
        .filter(|i| {
            let [r, g, b] = [0, 1, 2].map(|k| u16::from(c.color[i * 4 + k]));
            r > 2 * g + 20 && b > 2 * g + 20
        })
        .count()
}

/// **The pack's pictures take the place of the retail pictures it has materials for, while the
/// materials' switch is on**: along a Holtburg street, the houses' stone, timbers, plaster and
/// roofs are drawn magenta with the switch on, and as the frame with no pack draws them with it
/// off.
#[test]
fn the_packs_pictures_take_the_place_of_retail_pictures_while_its_materials_are_on() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = street();
    let pack = magenta_pack();
    let none = run(&store, &st, None, RemasterPacks::default(), 6);
    let materials = RemasterPacks {
        materials: true,
        ..RemasterPacks::default()
    };
    let on = run(&store, &st, Some((Arc::clone(&pack), None)), materials, 6);
    let trees_only = run(&store, &st, Some((pack, None)), trees(), 6);
    eprintln!(
        "street: magenta pixels {} with the pack's materials, {} without",
        magenta(&on.capture),
        magenta(&none.capture)
    );
    assert!(
        magenta(&on.capture) > 20_000,
        "the houses are drawn with the pack's pictures"
    );
    assert_eq!(magenta(&none.capture), 0);
    assert_eq!(
        trees_only.capture.color, none.capture.color,
        "the materials' switch off"
    );
}

/// `files` with each card's material given a normal picture of `normal` (a 16 by 16 picture of
/// one normal per column pair, in tangent space, each part -1..1 as 0..255).
fn with_normals(
    mut files: HashMap<String, Vec<u8>>,
    normal: &dyn Fn(u32) -> [u8; 4],
) -> HashMap<String, Vec<u8>> {
    let mut texels = Vec::new();
    for _ in 0..16u32 {
        for x in 0..16u32 {
            texels.extend(normal(x / 2));
        }
    }
    for level in ["near", "far"] {
        let path = format!("{level}/card.gltf");
        let gltf = String::from_utf8(files[&path].clone())
            .expect("text")
            .replacen(
                r#""metallicFactor": 0.0}}"#,
                r#""metallicFactor": 0.0}, "normalTexture": {"index": 1}}"#,
                1,
            )
            .replacen(
                r#""textures": [{"source": 0}]"#,
                r#""textures": [{"source": 0}, {"source": 1}]"#,
                1,
            )
            .replacen(
                r#""images": [{"uri": "leaf.png"}]"#,
                r#""images": [{"uri": "leaf.png"}, {"uri": "normal.png"}]"#,
                1,
            );
        assert!(gltf.contains("normal.png") && gltf.contains("normalTexture"));
        files.insert(path, gltf.into_bytes());
        files.insert(format!("{level}/normal.png"), png(16, 16, &texels));
    }
    files
}

/// The pixels of the world drawn whose color differs between `a` and `b`.
fn differ(a: &RemasterCapture, b: &RemasterCapture) -> usize {
    (0..a.class.len())
        .filter(|i| a.class[*i] != 0 && a.color[i * 4..i * 4 + 3] != b.color[i * 4..i * 4 + 3])
        .count()
}

/// **A pack model's normal picture bends its light**: in the forest, with the light per pixel,
/// the cards drawn with a normal picture whose columns lean each way differ from the cards drawn
/// with none at thousands of pixels; with a normal picture that leans nowhere, at almost none.
#[test]
fn a_pack_models_normal_picture_bends_its_light() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let plain = run(
        &store,
        &st,
        Some((in_memory(&files(None)), None)),
        trees(),
        6,
    );
    let leaning = with_normals(files(None), &|k| {
        if k % 2 == 0 {
            [230, 128, 180, 255]
        } else {
            [26, 128, 180, 255]
        }
    });
    let bent = run(&store, &st, Some((in_memory(&leaning), None)), trees(), 6);
    let flat = with_normals(files(None), &|_| [128, 128, 255, 255]);
    let level = run(&store, &st, Some((in_memory(&flat), None)), trees(), 6);
    let (by_bent, by_flat) = (
        differ(&bent.capture, &plain.capture),
        differ(&level.capture, &plain.capture),
    );
    eprintln!("forest: {by_bent} pixels differ with leaning normals, {by_flat} with flat ones");
    assert!(
        by_bent > 1_000,
        "the normal picture bends the light: {by_bent}"
    );
    assert!(
        by_flat * 20 < by_bent,
        "a flat normal picture barely: {by_flat}"
    );
}

/// **The pack's ground is laid behind retail's blend, while its switch is on**: in the forest,
/// with the ground drawn from its splats, a pack that lays every dry terrain type and the road in
/// magenta turns the ground magenta with its ground's switch on, and leaves it retail's with the
/// switch off, or with the ground drawn from its composites.
#[test]
fn the_packs_ground_is_laid_behind_retails_blend_while_its_switch_is_on() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let credit = "author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n";
    let mut manifest = String::from("[pack]\nname = \"Ground\"\nformat = 1\n");
    let types: Vec<String> = (0..16).chain(21..32).map(|t| t.to_string()).collect();
    manifest.push_str(&format!(
        "[[terrain]]\nground = [{}, \"road\"]\nbase_color = \"g.png\"\nsize = 4\n{credit}",
        types.join(", ")
    ));
    let picture = png(4, 4, &[255, 0, 255, 255].repeat(16));
    let pack = Arc::new(
        ArtPack::open(&manifest, &mut |path| {
            (path == "g.png")
                .then(|| picture.clone())
                .ok_or_else(|| format!("{path}: missing"))
        })
        .expect("the pack opens"),
    );
    let ground = RemasterPacks {
        ground: true,
        ..RemasterPacks::default()
    };
    let magenta_ground = |c: &RemasterCapture| {
        (0..c.class.len())
            .filter(|i| c.class[*i] == 1)
            .filter(|i| {
                let [r, g, b] = [0, 1, 2].map(|k| u16::from(c.color[i * 4 + k]));
                r > 2 * g + 20 && b > 2 * g + 20
            })
            .count()
    };
    let splats = |pack: Option<Arc<ArtPack>>, packs: RemasterPacks, splat: bool| {
        run_splat(&store, &st, pack, packs, splat)
    };
    let on = splats(Some(Arc::clone(&pack)), ground, true);
    let off = splats(Some(Arc::clone(&pack)), RemasterPacks::default(), true);
    let composites = splats(Some(pack), ground, false);
    let ground_px = on.class.iter().filter(|c| **c == 1).count();
    eprintln!(
        "forest: magenta ground {} of {ground_px} with the pack's ground, {} switched off, {} on \
         the composites",
        magenta_ground(&on),
        magenta_ground(&off),
        magenta_ground(&composites)
    );
    assert!(
        magenta_ground(&on) * 2 > ground_px,
        "the pack's ground is laid"
    );
    assert_eq!(magenta_ground(&off), 0);
    assert_eq!(magenta_ground(&composites), 0);
}

/// **What a pack lays on steep ground has its own switch**: on the hillside south of Holtburg, a
/// pack whose ground is white and whose material for its steep ground is magenta lays the magenta
/// on what is steep while the slopes' switch is on, on none of it with the switch off, and leaves
/// the flat ground white either way.
#[test]
fn the_packs_steep_ground_is_laid_while_its_switch_is_on() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "hillside");
    let credit = "author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n";
    let types: Vec<String> = (0..16).chain(21..32).map(|t| t.to_string()).collect();
    let manifest = format!(
        "[pack]\nname = \"Steep\"\nformat = 1\n\
         [[terrain]]\nground = [{0}, \"road\"]\nbase_color = \"flat.png\"\nsize = 4\n{credit}\
         [[steep]]\nground = [{0}]\nbase_color = \"steep.png\"\nsize = 4\n{credit}",
        types.join(", ")
    );
    let flat = png(4, 4, &[255, 255, 255, 255].repeat(16));
    let steep = png(4, 4, &[255, 0, 255, 255].repeat(16));
    let pack = Arc::new(
        ArtPack::open(&manifest, &mut |path| match path {
            "flat.png" => Ok(flat.clone()),
            "steep.png" => Ok(steep.clone()),
            _ => Err(format!("{path}: missing")),
        })
        .expect("the pack opens"),
    );
    let draw = |slopes: bool| {
        let mut gpu = device();
        let mut shot = Shot::open(&store, &mut gpu, &st);
        shot.poll(&mut gpu);
        shot.scene
            .draw
            .set_remaster_pack(Some(Arc::clone(&pack)), None);
        shot.scene.draw.set_remaster_packs(
            RemasterPacks {
                ground: true,
                slopes,
                ..RemasterPacks::default()
            },
            &mut gpu,
        );
        shot.scene.draw.set_remaster_terrain(
            dereth_scene::world_scene::RemasterTerrain::player(),
            &mut gpu,
        );
        shot.scene
            .draw
            .set_remaster(true, &store, &mut gpu)
            .expect("the remaster starts");
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_ground();
        for _ in 0..4 {
            shot.step_held(&mut gpu);
        }
        let c = shot
            .scene
            .draw
            .remaster_capture(&mut gpu)
            .expect("the remaster drew");
        shot.scene
            .draw
            .set_remaster(false, &store, &mut gpu)
            .expect("the remaster stops");
        c
    };
    let magenta = |c: &RemasterCapture| {
        (0..c.class.len())
            .filter(|i| c.class[*i] == 1)
            .filter(|i| {
                let [r, g, b] = [0, 1, 2].map(|k| u16::from(c.color[i * 4 + k]));
                r > 2 * g + 20 && b > 2 * g + 20
            })
            .count()
    };
    let (on, off) = (draw(true), draw(false));
    let ground = on.class.iter().filter(|c| **c == 1).count();
    eprintln!(
        "hillside: magenta {} of {ground} ground pixels with the switch on, {} off",
        magenta(&on),
        magenta(&off)
    );
    assert!(
        magenta(&on) > 200,
        "{} pixels of the steep ground",
        magenta(&on)
    );
    assert!(magenta(&on) < ground, "the flat ground stays its own");
    assert_eq!(magenta(&off), 0, "off, none of it");
}

/// How much a steep material's picture changes from one pixel to the next down the screen, against
/// its mean, on the steep faces drawn in `c`, whose material `steep` tells: seen square on, that is
/// down the fall line. And how many pixels were measured.
fn detail_down_the_fall_line(
    c: &RemasterCapture,
    steep: &dyn Fn(&[f32; 3]) -> bool,
) -> (f32, usize) {
    let (w, h) = (c.width as usize, c.height as usize);
    let face = |i: usize| c.class[i] == 1 && c.normal[i][2] < 0.6 && steep(&c.albedo[i]);
    let (mut change, mut sum, mut count) = (0.0, 0.0, 0usize);
    for y in 0..h.saturating_sub(1) {
        for x in 0..w {
            let (a, b) = (y * w + x, (y + 1) * w + x);
            if face(a) && face(b) {
                change += (c.albedo[a][0] - c.albedo[b][0]).abs();
                sum += c.albedo[a][0];
                count += 1;
            }
        }
    }
    (change / sum.max(1e-3), count)
}

/// Across the water from a bare cliff north of Holtburg, looking square at its face.
fn cliff() -> Station {
    Station {
        name: "cliff",
        why: "a steep face seen square on",
        place: Place::Free {
            block: 0xA9B6,
            x: 108.0,
            y: 150.0,
            eye: 3.0,
            yaw: 0.0,
            pitch: 0.12,
        },
        ..street()
    }
}

/// **A steep material keeps its detail down the fall line**: on a bare cliff, a pack's steep
/// material of fine speckle changes from pixel to pixel down the screen. Read through an even
/// filter, its picture is blurred by the pixel's long reach across the fall line, and the speckle
/// runs together into streaks down the face.
///
/// Behaviour: remaster.art-pack.remaster-steep-material-keeps-its-detail
#[test]
fn a_steep_material_keeps_its_detail_down_the_fall_line() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = cliff();
    let credit = "author = \"a\"
tool = \"t\"
license = \"CC0-1.0\"
";
    let types: Vec<String> = (0..16).chain(21..32).map(|t| t.to_string()).collect();
    let manifest = format!(
        "[pack]
name = \"Speckle\"
format = 1
         [[terrain]]
ground = [{0}, \"road\"]
base_color = \"flat.png\"
size = 4
{credit}         [[steep]]
ground = [{0}]
base_color = \"steep.png\"
size = 4
{credit}",
        types.join(", ")
    );
    let flat = png(4, 4, &[255, 255, 255, 255].repeat(16));
    // Magenta, of a brightness that changes at random from texel to texel.
    let mut word = 0x2545_F491u32;
    let speckle: Vec<u8> = (0..64 * 64)
        .flat_map(|_| {
            word ^= word << 13;
            word ^= word >> 17;
            word ^= word << 5;
            let v = 40 + (word >> 24) as u8 / 2;
            [v, 0, v, 255]
        })
        .collect();
    let steep = png(64, 64, &speckle);
    let pack = Arc::new(
        ArtPack::open(&manifest, &mut |path| match path {
            "flat.png" => Ok(flat.clone()),
            "steep.png" => Ok(steep.clone()),
            _ => Err(format!("{path}: missing")),
        })
        .expect("the pack opens"),
    );
    let mut gpu = device();
    let mut shot = Shot::open(&store, &mut gpu, &st);
    shot.poll(&mut gpu);
    shot.scene
        .draw
        .set_remaster_pack(Some(Arc::clone(&pack)), None);
    shot.scene.draw.set_remaster_packs(
        RemasterPacks {
            ground: true,
            slopes: true,
            ..RemasterPacks::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_terrain(
        dereth_scene::world_scene::RemasterTerrain::player(),
        &mut gpu,
    );
    shot.scene
        .draw
        .set_remaster(true, &store, &mut gpu)
        .expect("the remaster starts");
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..4 {
        shot.step_held(&mut gpu);
    }
    let c = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, &store, &mut gpu)
        .expect("the remaster stops");
    let (detail, pixels) = detail_down_the_fall_line(&c, &|a| a[1] < 0.5 * a[0]);
    eprintln!("the steep material's change down the screen: {detail:.4} over {pixels} pixels");
    assert!(pixels > 200, "steep faces in view: {pixels}");
    // Read through an even filter it comes to 0.026; through the long reach, 0.034.
    assert!(
        detail > 0.03,
        "the speckle still shows down the face: {detail}"
    );
}

/// The remaster at `station`, in linear light with the light per pixel: `pack` given, and its trees
/// drawn when `drawn`. A pack of no trees changes nothing it draws: only the look it brings.
fn run_look(
    store: &Arc<RetailDatStore>,
    station: &Station,
    pack: Option<Arc<ArtPack>>,
    drawn: bool,
) -> RemasterCapture {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(pack, None);
    shot.scene.draw.set_remaster_packs(
        RemasterPacks {
            trees: drawn,
            ..RemasterPacks::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_post(
        RemasterPost {
            hdr: true,
            ..RemasterPost::default()
        },
        &mut gpu,
    );
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
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..5 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    capture
}

/// A frame's mean saturation, as a share of each pixel's brightest channel, over `which` pixels.
fn saturation(c: &RemasterCapture, which: &dyn Fn(usize) -> bool) -> f32 {
    let (mut sum, mut count) = (0.0, 0u32);
    for i in (0..c.class.len()).filter(|i| which(*i)) {
        let [r, g, b] = [0, 1, 2].map(|k| f32::from(c.color[i * 4 + k]));
        let most = r.max(g).max(b);
        if most > 8.0 {
            sum += (most - r.min(g).min(b)) / most;
            count += 1;
        }
    }
    sum / count.max(1) as f32
}

/// A frame's mean brightness, 0 to 255, over `which` pixels.
fn brightness(c: &RemasterCapture, which: &dyn Fn(usize) -> bool) -> f32 {
    let (mut sum, mut count) = (0.0, 0u32);
    for i in (0..c.class.len()).filter(|i| which(*i)) {
        let [r, g, b] = [0, 1, 2].map(|k| f32::from(c.color[i * 4 + k]));
        sum += 0.2126 * r + 0.7152 * g + 0.0722 * b;
        count += 1;
    }
    sum / count.max(1) as f32
}

/// **An art pack brings a grade, and no pack brings none**: in the forest, an art pack drawn lays a
/// little more saturation over the frame and darkens its corners against its middle; with the
/// pack's switches on and no pack given, the frame is what it is with no art pack at all.
///
/// Behaviour: remaster.art-pack.remaster-grade
#[test]
fn an_art_pack_brings_a_grade_and_no_pack_brings_none() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let plain = run_look(&store, &st, None, false);
    let unpacked = run_look(&store, &st, None, true);
    let graded = run_look(&store, &st, Some(ground_pack(None, None)), true);
    let (w, h) = (plain.width as usize, plain.height as usize);
    let scene = |c: &RemasterCapture| {
        let class = c.class.clone();
        move |i: usize| class[i] != 0
    };
    let corner = move |i: usize| {
        let (x, y) = (i % w, i / w);
        (x < w / 8 || x >= w - w / 8) && (y < h / 8 || y >= h - h / 8)
    };
    let middle = move |i: usize| {
        let (x, y) = (i % w, i / w);
        x >= w * 3 / 8 && x < w * 5 / 8 && y >= h * 3 / 8 && y < h * 5 / 8
    };
    let (a, b) = (
        saturation(&plain, &scene(&plain)),
        saturation(&graded, &scene(&graded)),
    );
    let (kept, darker) = (
        brightness(&graded, &corner) / brightness(&plain, &corner).max(1.0),
        brightness(&graded, &middle) / brightness(&plain, &middle).max(1.0),
    );
    eprintln!(
        "saturation {a:.3} to {b:.3}; the corners keep {kept:.3}, the middle {darker:.3}; \
         {} pixels differ with no pack",
        differ(&plain, &unpacked)
    );
    assert!(b > a * 1.02, "the grade saturates the frame: {a} to {b}");
    assert!(
        kept < darker,
        "the vignette darkens the corners: {kept} against {darker}"
    );
    assert!(
        differ(&plain, &unpacked) < plain.class.len() / 200,
        "no pack, no look"
    );
}

/// How far a frame's `which` pixels spread in brightness: the brightest tenth over the darkest.
fn spread(c: &RemasterCapture, which: &dyn Fn(usize) -> bool) -> f32 {
    let mut seen: Vec<f32> = (0..c.class.len())
        .filter(|i| which(*i))
        .map(|i| {
            let [r, g, b] = [0, 1, 2].map(|k| f32::from(c.color[i * 4 + k]));
            0.2126 * r + 0.7152 * g + 0.0722 * b
        })
        .collect();
    seen.sort_by(f32::total_cmp);
    let n = seen.len();
    if n < 10 {
        return 0.0;
    }
    seen[n * 9 / 10] / seen[n / 10].max(1.0)
}

/// **An art pack's light is softer on what the sun only grazes**: on a bare cliff at midday, with an
/// art pack drawn, steep ground gains more than flat ground; the walls of a street are lit more than
/// retail lights them; and the leaves of the trees in the forest come out no paler than retail's,
/// holding the depth between what the sun finds and what lies in shade.
///
/// Behaviour: remaster.art-pack.remaster-light
#[test]
fn an_art_packs_light_is_softer_on_what_the_sun_only_grazes() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let look = || Some(ground_pack(None, None));
    let plain = run_look(&store, &cliff(), None, false);
    let lit = run_look(&store, &cliff(), look(), true);
    let gain = |steep: bool| {
        let which = |i: usize| {
            plain.class[i] == 1
                && if steep {
                    plain.normal[i][2] < 0.7
                } else {
                    plain.normal[i][2] > 0.95
                }
        };
        brightness(&lit, &which) / brightness(&plain, &which).max(1.0)
    };
    let (face, flat) = (gain(true), gain(false));
    let of_kind = |c: &RemasterCapture, foliage: bool| {
        brightness(c, &|i| {
            c.class[i] > 1 && (c.kind[i] == 1) == foliage && c.kind[i] != 0
        })
    };
    let forest = station(&store, "forest");
    let (bare, packed) = (
        run_look(&store, &forest, None, false),
        run_look(&store, &forest, look(), true),
    );
    let leaf_depth = |c: &RemasterCapture| spread(c, &|i| c.class[i] > 1 && c.kind[i] == 1);
    let leaves = (of_kind(&bare, true), of_kind(&packed, true));
    let depth = (leaf_depth(&bare), leaf_depth(&packed));
    let walls = |c: &RemasterCapture| brightness(c, &|i| c.class[i] > 1 && c.kind[i] != 1);
    let town = (
        walls(&run_look(&store, &street(), None, false)),
        walls(&run_look(&store, &street(), look(), true)),
    );
    eprintln!(
        "steep ground gains {face:.3}, flat ground {flat:.3}; the leaves {:.1} to {:.1}, their \
         spread {:.2} to {:.2}; the walls {:.1} to {:.1}",
        leaves.0, leaves.1, depth.0, depth.1, town.0, town.1
    );
    assert!(
        face > flat + 0.05,
        "steep ground gains more: {face} against {flat}"
    );
    assert!(
        leaves.1 < leaves.0 * 1.1 && depth.1 > depth.0 * 0.85,
        "the leaves stay as deep as retail's: {leaves:?} {depth:?}"
    );
    assert!(town.1 > town.0 * 1.05, "the walls are lit more: {town:?}");
}

/// How unevenly the light falls on the steep faces drawn in `c`, whatever their pictures: the
/// brightest tenth of their pixels' light over their darkest tenth, and how many were measured.
/// Dark streaks down a fold in the ground are what spread it.
fn streaks_on_the_faces(c: &RemasterCapture) -> (f32, usize) {
    let mut light: Vec<f32> = (0..c.class.len())
        .filter(|i| c.class[*i] == 1 && c.normal[*i][2] < 0.7)
        .filter_map(|i| {
            let [r, g, b] = [0, 1, 2].map(|k| f32::from(c.color[i * 4 + k]) / 255.0);
            let shown = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let [a, d, e] = c.albedo[i];
            let own = 0.2126 * a + 0.7152 * d + 0.0722 * e;
            (own > 0.02).then_some(shown / own)
        })
        .collect();
    light.sort_by(f32::total_cmp);
    let n = light.len();
    if n < 10 {
        return (0.0, n);
    }
    (light[n * 9 / 10] / light[n / 10].max(1e-3), n)
}

/// **Folds in a cliff do not read as dark streaks**: on a bare cliff at midday, with an art pack
/// drawn, the light on the steep faces stays even, where the folds down the fall line turn each
/// from the sun in turn. The open sky lights what the sun misses, as Retail Look's flat ambient
/// does not.
///
/// Behaviour: remaster.art-pack.remaster-sky-lights-what-the-sun-misses
#[test]
fn folds_in_a_cliff_do_not_read_as_dark_streaks() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let lit = run_look(&store, &cliff(), Some(ground_pack(None, None)), true);
    let (spread, pixels) = streaks_on_the_faces(&lit);
    eprintln!("the light on the faces spreads {spread:.3} over {pixels} pixels");
    assert!(pixels > 2000, "steep faces in view: {pixels}");
    // With no sky but retail's ambient it comes to 1.42; with the open sky, 1.20.
    assert!(spread < 1.3, "the faces still streak: {spread}");
}

/// [`run`] with the ground drawn from its splats when `splat` says.
fn run_splat(
    store: &Arc<RetailDatStore>,
    station: &Station,
    pack: Option<Arc<ArtPack>>,
    packs: RemasterPacks,
    splat: bool,
) -> RemasterCapture {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(pack, None);
    shot.scene.draw.set_remaster_packs(packs, &mut gpu);
    shot.scene.draw.set_remaster_terrain(
        dereth_scene::world_scene::RemasterTerrain {
            splat,
            ..Default::default()
        },
        &mut gpu,
    );
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    for _ in 0..6 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    capture
}

/// `files` with each card's material given a metal-roughness picture all `texel` (glTF's: the
/// roughness in green, the metalness in blue).
fn with_metal_rough(
    mut files: HashMap<String, Vec<u8>>,
    texel: [u8; 4],
) -> HashMap<String, Vec<u8>> {
    for level in ["near", "far"] {
        let path = format!("{level}/card.gltf");
        let gltf = String::from_utf8(files[&path].clone())
            .expect("text")
            .replacen(
                r#""metallicFactor": 0.0}"#,
                r#""metallicFactor": 0.0, "metallicRoughnessTexture": {"index": 1}}"#,
                1,
            )
            .replacen(
                r#""textures": [{"source": 0}]"#,
                r#""textures": [{"source": 0}, {"source": 1}]"#,
                1,
            )
            .replacen(
                r#""images": [{"uri": "leaf.png"}]"#,
                r#""images": [{"uri": "leaf.png"}, {"uri": "rough.png"}]"#,
                1,
            );
        assert!(gltf.contains("rough.png") && gltf.contains("metallicRoughnessTexture"));
        files.insert(path, gltf.into_bytes());
        files.insert(format!("{level}/rough.png"), png(4, 4, &texel.repeat(16)));
    }
    files
}

/// `files` with a gray leaf in place of the green, its parts drawn under `vertex` (every corner's
/// `COLOR_0`) and `factor` (the material's base color factor), both in linear light.
fn gray_with_color(vertex: Option<[f32; 3]>, factor: Option<[f32; 3]>) -> HashMap<String, Vec<u8>> {
    let mut files = files(None);
    for level in ["near", "far"] {
        let leaf: Vec<u8> = files[&format!("{level}/leaf.png")].clone();
        let (_, _, rgba) = dereth_scene::art_pack::images::decode_png(&leaf).expect("the leaf");
        let gray: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|t| if t[3] == 0 { *t } else { [200, 200, 200, 255] })
            .collect();
        files.insert(format!("{level}/leaf.png"), png(16, 16, &gray));
        let path = format!("{level}/card.gltf");
        let mut gltf = String::from_utf8(files[&path].clone()).expect("text");
        if let Some([r, g, b]) = factor {
            gltf = gltf.replacen(
                r#""metallicFactor": 0.0}"#,
                &format!(r#""metallicFactor": 0.0, "baseColorFactor": [{r}, {g}, {b}, 1.0]}}"#),
                1,
            );
            assert!(gltf.contains("baseColorFactor"));
        }
        if let Some(color) = vertex {
            gltf = gltf
                .replacen(
                    r#""TEXCOORD_0": 2}"#,
                    r#""TEXCOORD_0": 2, "COLOR_0": 4}"#,
                    1,
                )
                .replacen(r#""byteLength": 152}"#, r#""byteLength": 200}"#, 1)
                .replacen(
                    r#""byteOffset": 128, "byteLength": 24}]"#,
                    r#""byteOffset": 128, "byteLength": 24},
                    {"buffer": 0, "byteOffset": 152, "byteLength": 48}]"#,
                    1,
                )
                .replacen(
                    r#""count": 6, "type": "SCALAR"}]"#,
                    r#""count": 6, "type": "SCALAR"},
                    {"bufferView": 4, "componentType": 5126, "count": 4, "type": "VEC3"}]"#,
                    1,
                );
            assert!(gltf.contains("COLOR_0") && gltf.contains(r#""byteLength": 200"#));
            let bin = files
                .get_mut(&format!("{level}/card.bin"))
                .expect("the bin");
            for _ in 0..4 {
                for c in color {
                    bin.extend(c.to_le_bytes());
                }
            }
        }
        files.insert(path, gltf.into_bytes());
    }
    files
}

/// **A pack's green comes from its vertices and its factor, not only its picture**: in the
/// forest, cards with a neutral gray leaf are drawn gray, and drawn green once their vertices or
/// their material's factor carry the green.
///
/// Behaviour: remaster.art-pack.remaster-draws-a-models-vertex-color-and-color-factor
#[test]
fn a_gray_leaf_is_drawn_under_its_vertices_color_and_its_materials_factor() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let green = [0.02, 0.60, 0.02];
    let draw = |vertex, factor| {
        let pack = in_memory(&gray_with_color(vertex, factor));
        let f = run(&store, &st, Some((pack, None)), trees(), 6);
        assert!(f.counts.placements > 0, "the cards are drawn");
        leafy(&f.capture)
    };
    let gray = draw(None, None);
    let by_vertex = draw(Some(green), None);
    let by_factor = draw(None, Some(green));
    let none = run(&store, &st, None, RemasterPacks::default(), 6);
    let retail = leafy(&none.capture);
    assert!(
        by_vertex > gray + 2000 && by_vertex > retail,
        "green vertices: {by_vertex} leaf-green pixels, against {gray} gray, {retail} retail"
    );
    assert!(
        by_factor > gray + 2000 && by_factor > retail,
        "a green factor: {by_factor} leaf-green pixels, against {gray} gray, {retail} retail"
    );
}

/// The roughness the G-buffer holds at the pixels `which` picks, the middle of them; `None` where
/// it picks none.
fn roughness_at(c: &RemasterCapture, which: &dyn Fn(usize) -> bool) -> Option<u8> {
    let mut r: Vec<u8> = (0..c.roughness.len())
        .filter(|i| which(*i))
        .map(|i| c.roughness[i])
        .collect();
    r.sort_unstable();
    r.get(r.len() / 2).copied()
}

/// **A pack's roughness reaches the G-buffer**: along a Holtburg street, the houses drawn with a
/// pack material whose roughness picture is 0.2 hold 0.2 as their roughness in the G-buffer; in
/// the forest, the cards whose metal-roughness picture holds 0.25 in its green hold 0.25, and
/// cards with no picture and a roughness factor of a half hold a half; and retail's walls hold
/// 1.
#[test]
fn a_packs_roughness_reaches_the_g_buffer() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let materials = RemasterPacks {
        materials: true,
        ..RemasterPacks::default()
    };
    let pack = magenta_pack_of(Some(51));
    let street_rough = run(&store, &street(), Some((pack, None)), materials, 6);
    let c = &street_rough.capture;
    let pink = |i: usize| {
        let [r, g, b] = [0, 1, 2].map(|k| u16::from(c.color[i * 4 + k]));
        c.class[i] != 0 && r > 2 * g + 20 && b > 2 * g + 20
    };
    let none = run(&store, &street(), None, RemasterPacks::default(), 6);
    let n = &none.capture;
    let st = station(&store, "forest");
    let rough = with_metal_rough(files(None), [255, 64, 0, 255]);
    let forest = run(&store, &st, Some((in_memory(&rough), None)), trees(), 6);
    let f = &forest.capture;
    let leaf = |i: usize| {
        let p = &f.color[i * 4..i * 4 + 3];
        f.class[i] != 0
            && u16::from(p[1]) > u16::from(p[0]) * 2
            && u16::from(p[1]) > u16::from(p[2]) * 2
    };
    let factor = files(None)
        .into_iter()
        .map(|(k, v)| {
            if k.ends_with(".gltf") {
                let g = String::from_utf8(v).expect("text").replacen(
                    r#""metallicFactor": 0.0}"#,
                    r#""metallicFactor": 0.0, "roughnessFactor": 0.5}"#,
                    1,
                );
                assert!(g.contains("roughnessFactor"));
                (k, g.into_bytes())
            } else {
                (k, v)
            }
        })
        .collect();
    let halves = run(&store, &st, Some((in_memory(&factor), None)), trees(), 6);
    let h = &halves.capture;
    let half_leaf = |i: usize| {
        let p = &h.color[i * 4..i * 4 + 3];
        h.class[i] != 0
            && u16::from(p[1]) > u16::from(p[0]) * 2
            && u16::from(p[1]) > u16::from(p[2]) * 2
    };
    let by_factor = roughness_at(h, &half_leaf);
    let (houses, cards) = (roughness_at(c, &pink), roughness_at(f, &leaf));
    let retail = roughness_at(n, &|i| n.class[i] == 2 && n.kind[i] == 4);
    eprintln!(
        "roughness: houses {houses:?} of 51, cards {cards:?} of 64, by the factor {by_factor:?} \
         of 128, retail's walls {retail:?}"
    );
    assert!(
        by_factor.is_some_and(|r| r.abs_diff(128) <= 2),
        "the factor's: {by_factor:?}"
    );
    assert!(
        houses.is_some_and(|r| r.abs_diff(51) <= 2),
        "the material's: {houses:?}"
    );
    assert!(
        cards.is_some_and(|r| r.abs_diff(64) <= 2),
        "the model's: {cards:?}"
    );
    assert_eq!(retail, Some(255), "retail's");
}

/// A pack laying every dry terrain type and the road with one gray ground, given, when they are,
/// a normal picture all `normal` (its lean east and north, each -1..1 as 0..255) and a roughness
/// picture all `roughness`.
fn ground_pack(normal: Option<[u8; 2]>, roughness: Option<u8>) -> Arc<ArtPack> {
    let types: Vec<String> = (0..16).chain(21..32).map(|t| t.to_string()).collect();
    let mut manifest = format!(
        "[pack]\nname = \"Ground\"\nformat = 1\n[[terrain]]\nground = [{}, \"road\"]\n\
         base_color = \"g.png\"\nsize = 4\nauthor = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n",
        types.join(", ")
    );
    if normal.is_some() {
        manifest.push_str("normal = \"n.png\"\n");
    }
    if roughness.is_some() {
        manifest.push_str("roughness = \"r.png\"\n");
    }
    let gray = png(4, 4, &[150, 150, 150, 255].repeat(16));
    let lean = normal.map(|[e, n]| png(4, 4, &[e, n, 255, 255].repeat(16)));
    let rough = roughness.map(|r| png(4, 4, &[r, r, r, 255].repeat(16)));
    Arc::new(
        ArtPack::open(&manifest, &mut |path| match path {
            "g.png" => Ok(gray.clone()),
            "n.png" => lean.clone().ok_or_else(|| format!("{path}: none")),
            "r.png" => rough.clone().ok_or_else(|| format!("{path}: none")),
            _ => Err(format!("{path}: missing")),
        })
        .expect("the pack opens"),
    )
}

/// The remaster at `station`, its light per pixel, with `pack`'s ground drawn (when there is a
/// pack) on the ground as `ground` draws it, every ground cell turned `turn` more quarter turns
/// than its own; the last of six frames held still.
fn run_ground(
    store: &Arc<RetailDatStore>,
    station: &Station,
    pack: Option<Arc<ArtPack>>,
    (ground, turn): (dereth_scene::world_scene::RemasterTerrain, u8),
) -> RemasterCapture {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(pack, None);
    shot.scene.draw.set_remaster_packs(
        RemasterPacks {
            ground: true,
            ..RemasterPacks::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_terrain(ground, &mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.scene.draw.set_remaster_turned_cells(&mut gpu, turn);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..5 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    capture
}

/// The ground pixels of `a` whose color differs from `b`'s by more than a few steps.
fn ground_differs(a: &RemasterCapture, b: &RemasterCapture) -> usize {
    (0..a.class.len())
        .filter(|i| a.class[*i] == 1 && b.class[*i] == 1)
        .filter(|i| (0..3).any(|k| a.color[i * 4 + k].abs_diff(b.color[i * 4 + k]) > 3))
        .count()
}

/// **The pack ground's normal and roughness pictures are laid with its tiles**: in the forest,
/// its light per pixel, on retail's grid and on the grid of patches, a ground whose normal
/// picture leans east lights the ground differently from the same ground with a level normal
/// picture at most of its pixels, where the level picture and none at all differ at almost
/// none; and the ground's roughness in the G-buffer is the roughness picture's.
#[test]
fn the_pack_grounds_normal_and_roughness_pictures_are_laid_with_its_tiles() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let grids = [
        (
            "retail's grid",
            dereth_scene::world_scene::RemasterTerrain {
                splat: true,
                ..Default::default()
            },
        ),
        (
            "the grid of patches",
            dereth_scene::world_scene::RemasterTerrain::player(),
        ),
    ];
    for (name, grid) in grids {
        let leaning = run_ground(
            &store,
            &st,
            Some(ground_pack(Some([230, 128]), None)),
            (grid, 0),
        );
        let level = run_ground(
            &store,
            &st,
            Some(ground_pack(Some([128, 128]), None)),
            (grid, 0),
        );
        let none = run_ground(&store, &st, Some(ground_pack(None, Some(40))), (grid, 0));
        let ground = level.class.iter().filter(|c| **c == 1).count();
        let leans = ground_differs(&leaning, &level);
        let flat = ground_differs(&level, &none);
        let rough: Vec<u8> = (0..none.class.len())
            .filter(|i| none.class[*i] == 1)
            .map(|i| none.roughness[i])
            .collect();
        let near_40 = rough.iter().filter(|r| r.abs_diff(40) <= 8).count();
        eprintln!(
            "{name}: {ground} ground pixels; {leans} lit differently leaning east, {flat} with a \
             level normal picture against none; {near_40} at the roughness picture's 40"
        );
        assert!(ground > 10_000, "{name}: the ground is in view");
        assert!(
            leans * 2 > ground,
            "{name}: the leaning normal bends the light"
        );
        assert!(flat * 100 < ground, "{name}: a level normal leaves it");
        assert!(near_40 * 2 > rough.len(), "{name}: the roughness is laid");
    }
}

/// How the ground's normals lean in `a` against `b`, on the pixels both draw as ground: the mean
/// of the difference east and north.
fn mean_lean(a: &RemasterCapture, b: &RemasterCapture) -> (f32, f32) {
    let (mut sum, mut n) = ((0.0f32, 0.0f32), 0usize);
    for i in 0..a.class.len() {
        if a.class[i] != 1 || b.class[i] != 1 {
            continue;
        }
        sum.0 += a.normal[i][0] - b.normal[i][0];
        sum.1 += a.normal[i][1] - b.normal[i][1];
        n += 1;
    }
    #[allow(clippy::cast_precision_loss)] // pixels of a frame
    let n = n.max(1) as f32;
    (sum.0 / n, sum.1 / n)
}

/// **A ground layer's normal picture turns with its cell**: in the forest, on retail's grid and
/// on the grid of patches, a normal picture leaning down its rows leans the ground south where
/// the cells lie as retail lays them, and east where every cell is turned a quarter, as the cell's
/// picture is turned: never north, and never staying south. (Retail's own grounds that blend
/// layers lay every cell unturned; a turned cell is made for the check.)
#[test]
fn a_ground_layers_normal_picture_turns_with_its_cell() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let grids = [
        (
            "retail's grid",
            dereth_scene::world_scene::RemasterTerrain {
                splat: true,
                ..Default::default()
            },
        ),
        (
            "the grid of patches",
            dereth_scene::world_scene::RemasterTerrain::player(),
        ),
    ];
    for (name, grid) in grids {
        let down = || Some(ground_pack(Some([128, 26]), None));
        let level = || Some(ground_pack(Some([128, 128]), None));
        let mut leans = Vec::new();
        for turn in [0u8, 1] {
            let a = run_ground(&store, &st, down(), (grid, turn));
            let b = run_ground(&store, &st, level(), (grid, turn));
            let (east, north) = mean_lean(&a, &b);
            println!("{name}, turned {turn}: the normal leans {east:.3} east, {north:.3} north");
            leans.push((east, north));
        }
        let (east, north) = leans[0];
        assert!(
            north < -0.2 && east.abs() < north.abs() * 0.3,
            "{name}, as laid: leans south"
        );
        let (east, north) = leans[1];
        assert!(
            east > 0.2 && north.abs() < east * 0.3,
            "{name}, turned a quarter: leans east"
        );
    }
}

/// The far pictures' quad, sixteen meters across and twenty-four high, facing the eye; as glTF,
/// its height on the second axis.
fn far_quad() -> (String, Vec<u8>) {
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

/// The pack of cards, with the most common tree's far pictures too, sixteen views of green filling
/// the tile but its corners; or, with `pictures` false, its cards alone.
fn far_files(pictures: bool) -> HashMap<String, Vec<u8>> {
    let mut files = files(None);
    if !pictures {
        return files;
    }
    let (w, h) = (256u32, 128u32);
    let mut albedo = Vec::new();
    let mut normal = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let (tx, ty) = (x % 32, y % 64);
            let edge = tx.min(31 - tx) < 4 && ty.min(63 - ty) < 4;
            albedo.extend(if edge {
                [40, 200, 60, 0]
            } else {
                [40, 200, 60, 255]
            });
            normal.extend([128, 128, 255, 128]);
        }
    }
    files.insert("far/far_albedo.png".to_owned(), png(w, h, &albedo));
    files.insert("far/far_normal.png".to_owned(), png(w, h, &normal));
    let (gltf, bin) = far_quad();
    files.insert("far/far.gltf".to_owned(), gltf.into_bytes());
    files.insert("far/far.bin".to_owned(), bin);
    let manifest = String::from_utf8(files["manifest.toml"].clone())
        .expect("text")
        .replace(
            "file = \"far/card.gltf\"
",
            "file = \"far/card.gltf\"
distance = 300.0
",
        );
    files.insert(
        "manifest.toml".to_owned(),
        format!("{manifest}[[model.lod]]\nfile = \"far/far.gltf\"\nviews = 16\ncolumns = 8\n")
            .into_bytes(),
    );
    files
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

/// The remaster over the forest with `pack` drawn, its far trees and cover worked out and settled.
fn over_forest(
    store: &Arc<RetailDatStore>,
    pack: Arc<ArtPack>,
) -> (
    RemasterCapture,
    dereth_scene::world_scene::RemasterForestStats,
) {
    far_forest_at(store, Some(pack), &over_the_forest(), None)
}

/// The remaster at `station` with `pack` drawn (none where none is given) and the ground's knobs
/// `knobs` (each at its default where none are given), its far trees and cover worked out and
/// settled, and every far tree's pictures on the device.
fn far_forest_at(
    store: &Arc<RetailDatStore>,
    pack: Option<Arc<ArtPack>>,
    station: &Station,
    knobs: Option<dereth_terrain::land::drawn::Knobs>,
) -> (
    RemasterCapture,
    dereth_scene::world_scene::RemasterForestStats,
) {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, station);
    shot.poll(&mut gpu);
    if let Some(k) = knobs {
        shot.scene.set_remaster_ground_knobs(k);
    }
    if let Some(pack) = pack {
        shot.scene.draw.set_remaster_pack(Some(pack), None);
        shot.scene.draw.set_remaster_packs(trees(), &mut gpu);
    }
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.scene.draw.set_remaster_terrain(
        dereth_scene::world_scene::RemasterTerrain {
            splat: true,
            grid: true,
            drawn: true,
            horizon: true,
        },
        &mut gpu,
    );
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_far_trees();
    // The models' pictures come a few a frame, those still being drawn when the frame asks for
    // them on a later one.
    for _ in 0..400 {
        shot.step_held(&mut gpu);
        shot.scene.wait_for_remaster_far_trees();
        let s = shot.scene.draw.remaster_forest_stats(&mut gpu);
        if s.is_none_or(|s| s.wanted > 0 && s.pictures >= s.wanted) {
            break;
        }
    }
    for _ in 0..4 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    let stats = shot
        .scene
        .draw
        .remaster_forest_stats(&mut gpu)
        .expect("the remaster is on");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    (capture, stats)
}

/// **A pack's far pictures stand its trees out past the blocks round the viewer, and its forest
/// cover lies on the far ground**: over the forest, a pack that has the most common tree's far
/// pictures draws its pictures for that tree among thousands of far trees, and lays its forest
/// cover; the same pack without them leaves that tree to retail's own far pictures, so the far
/// forest stands as many trees, from as many models.
///
/// Behaviour: remaster.art-pack.remaster-draws-far-trees-and-forest-cover
#[test]
fn a_packs_far_pictures_stand_its_trees_past_the_blocks_round_the_viewer() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (far, stats) = over_forest(&store, in_memory(&far_files(true)));
    let (near, without) = over_forest(&store, in_memory(&far_files(false)));
    let (with_far, without_far) = (leafy(&far), leafy(&near));
    eprintln!("forest: {stats:?}; leaf-green {with_far}, {without_far} with no far pictures");
    assert!(
        stats.pictures >= stats.wanted && stats.wanted >= 1,
        "{stats:?}"
    );
    assert!(stats.standing > 1000, "{stats:?}");
    assert!(stats.cover, "the forest cover is worked out: {stats:?}");
    assert_eq!(without.models, stats.models, "{without:?}");
    assert_eq!(without.standing, stats.standing, "{without:?}");
    assert!(
        with_far > 2000 && without_far > 2000,
        "the far trees are drawn: {with_far} leaf-green pixels, and {without_far}"
    );
}

/// **Retail's own trees stand in the far forest with no art pack**: over the forest with no pack
/// at all, the far forest stands thousands of retail's trees, from far pictures drawn from their
/// own meshes, every standing model's pictures on the device; with the far trees switched off it
/// stands none, and the frame shows far fewer plants' pixels.
///
/// Behaviour: remaster.art-pack.remaster-draws-far-trees-and-forest-cover
#[test]
fn retails_own_trees_stand_in_the_far_forest_with_no_art_pack() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (with, stats) = far_forest_at(&store, None, &over_the_forest(), None);
    let (off, notes) = dereth_terrain::land::drawn::Knobs::parse("far_trees = off");
    assert!(notes.is_empty(), "{notes:?}");
    let (without, none) = far_forest_at(&store, None, &over_the_forest(), Some(off));
    // The plants' pixels lit outdoors.
    let leaves = |c: &RemasterCapture| {
        (0..c.kind.len())
            .filter(|i| c.class[*i] == 2 && c.kind[*i] == 1)
            .count()
    };
    let (with_leaf, without_leaf) = (leaves(&with), leaves(&without));
    eprintln!("forest, no pack: {stats:?}; plants {with_leaf}, {without_leaf} with no far trees");
    assert!(
        stats.models > 0,
        "retail's trees have far pictures: {stats:?}"
    );
    assert_eq!(
        stats.pictures, stats.wanted,
        "every far tree's pictures: {stats:?}"
    );
    assert!(stats.standing > 1000, "{stats:?}");
    assert_eq!(none.standing, 0, "{none:?}");
    assert!(
        with_leaf > without_leaf + 2000,
        "the far trees are drawn: {with_leaf} plants' pixels, against {without_leaf}"
    );
}

/// The pixels of what is drawn standing above the ground's outline, column by column, and of what
/// stands below it: the objects (lit by the sun) over the topmost ground in each column, and the
/// rest of them.
fn over_and_under_the_outline(c: &RemasterCapture) -> (usize, usize) {
    const GROUND: u8 = 1;
    const OBJECT: u8 = 2;
    let (w, h) = (c.width as usize, c.class.len() / c.width as usize);
    let (mut over, mut under) = (0, 0);
    for x in 0..w {
        let top = (0..h).find(|&y| c.class[y * w + x] == GROUND).unwrap_or(h);
        for y in 0..h {
            if c.class[y * w + x] == OBJECT {
                if y < top {
                    over += 1;
                } else {
                    under += 1;
                }
            }
        }
    }
    (over, under)
}

/// **A far tree standing on the skyline keeps a small part of its size**: looking out from the
/// highest ground near Holtburg, the far trees on the far mountains' tops stand against the sky;
/// drawn with the skyline's share, far fewer pixels of what stands above the ground's outline are
/// drawn than with every tree at its whole size, while the forest backed by the land below the
/// outline is drawn much as it was.
#[test]
fn a_far_tree_on_the_skyline_keeps_a_small_part_of_its_size() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let vista = station(&store, "vista");
    let whole = {
        let mut k = dereth_terrain::land::drawn::Knobs::default();
        k.set(dereth_terrain::land::drawn::Knob::TreeSkyline, 1.0);
        k
    };
    let pack = || in_memory(&far_files(true));
    let (shrunk, stats) = far_forest_at(&store, Some(pack()), &vista, None);
    let (kept, _) = far_forest_at(&store, Some(pack()), &vista, Some(whole));
    let (over, under) = over_and_under_the_outline(&shrunk);
    let (over_whole, under_whole) = over_and_under_the_outline(&kept);
    eprintln!(
        "{stats:?}; over the outline {over} pixels, {over_whole} with every far tree whole; \
         under it {under}, {under_whole}"
    );
    assert!(stats.standing > 1000, "{stats:?}");
    assert!(
        over * 2 < over_whole,
        "the skyline's trees are smaller: {over} pixels over the outline against {over_whole}"
    );
    #[allow(clippy::cast_precision_loss)] // pixel counts
    let kept_under = under as f32 / under_whole.max(1) as f32;
    assert!(
        kept_under > 0.85,
        "the forest backed by land is drawn as it was: {under} pixels against {under_whole}"
    );
}

/// A shore of the lake, the camera low by the water, a muddy bank before it.
const SHORE: &str = "dereth report 1; loc 0xA9B5000D [39.936409 110.799110 27.554998] 0.614589 \
                     0.000000 0.000000 -0.788848; camera 0xA9B5 [34.34513 112.209175 30.713133] \
                     yaw -1.8178362 pitch -0.28000048 cell 0xA9B5000D; time 0.781; clock \
                     306486365.4719249; weather auto";

/// The water's kind among the material's kinds.
const WATER_KIND: u8 = 7;

/// A pack that lays mud and marsh in red and every other dry terrain type in green.
fn bank_pack() -> Arc<ArtPack> {
    let dry: Vec<String> = (0..16)
        .chain(21..32)
        .filter(|t| !matches!(t, 4 | 5))
        .map(|t| t.to_string())
        .collect();
    let terrain = |ground: &str, picture: &str| {
        format!(
            "[[terrain]]\nground = [{ground}]\nbase_color = \"{picture}\"\nsize = 4\n\
             author = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n"
        )
    };
    let manifest = format!(
        "[pack]\nname = \"Banks\"\nformat = 1\n{}{}",
        terrain(&format!("{}, \"road\"", dry.join(", ")), "green.png"),
        terrain("4, 5", "red.png"),
    );
    let (green, red) = (
        png(4, 4, &[20, 200, 20, 128].repeat(16)),
        png(4, 4, &[200, 20, 20, 128].repeat(16)),
    );
    Arc::new(
        ArtPack::open(&manifest, &mut |path| match path {
            "green.png" => Ok(green.clone()),
            "red.png" => Ok(red.clone()),
            _ => Err(format!("{path}: missing")),
        })
        .expect("the pack opens"),
    )
}

/// The remaster on the shore, the player's ground and water drawn, with `pack`'s ground (when there
/// is a pack) and `green` the share of grass grown over a muddy bank.
fn run_bank(
    store: &Arc<RetailDatStore>,
    pack: Option<Arc<ArtPack>>,
    green: f32,
) -> RemasterCapture {
    use dereth_terrain::land::drawn::{Knob, Knobs};
    let report =
        dereth_client_runtime::report::VisualReport::parse(SHORE).expect("the report reads");
    let st = crate::instruments::stations::report_station(&report, Some(3));
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, &st);
    shot.apply_report(&report);
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(pack, None);
    shot.scene.draw.set_remaster_packs(
        RemasterPacks {
            ground: true,
            ..RemasterPacks::default()
        },
        &mut gpu,
    );
    let mut knobs = Knobs::default();
    knobs.set(Knob::BankGreen, green);
    shot.scene.set_remaster_ground_knobs(knobs);
    shot.scene.draw.set_remaster_water(
        dereth_scene::world_scene::RemasterWater {
            on: true,
            ..Default::default()
        },
        &mut gpu,
    );
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
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..4 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    capture
}

/// Where the pixel `i` of `c` stands, from its depth.
#[allow(clippy::cast_precision_loss)] // pixels of a frame
fn stands_at(c: &RemasterCapture, inv: &glam::Mat4, i: usize) -> glam::Vec3 {
    let (w, h) = (c.width as usize, c.height as usize);
    let (x, y) = (i % w, i / w);
    let ndc = glam::Vec3::new(
        (x as f32 + 0.5) / w as f32 * 2.0 - 1.0,
        1.0 - (y as f32 + 0.5) / h as f32 * 2.0,
        c.depth[i],
    );
    inv.project_point3(ndc)
}

/// **A pack's grass grows in over a muddy bank, away from the water**: on a lake's shore, with a
/// pack whose mud is red and whose other grounds are green, the land that is all mud with the
/// grass turned off takes the green in patches, far more of it well away from the water than near
/// its edge; retail's water ground is laid as the land and the bed, not as retail's water
/// picture; the wet ground at the edge is darker than the same materials dry. With no pack the
/// knob changes nothing.
///
/// Behaviour: remaster.art-pack.remaster-grows-grass-on-muddy-banks
#[test]
#[allow(clippy::cast_precision_loss)] // pixels of a frame
fn a_packs_grass_grows_over_a_muddy_bank_away_from_the_water() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let on = run_bank(&store, Some(bank_pack()), 0.7);
    let off = run_bank(&store, Some(bank_pack()), 0.0);
    let inv = glam::Mat4::from_cols_array(&off.view_projection).inverse();
    let waters: Vec<glam::Vec3> = (0..off.kind.len())
        .filter(|i| off.class[*i] == 1 && off.kind[*i] == WATER_KIND && i % 7 == 0)
        .map(|i| stands_at(&off, &inv, i))
        .collect();
    assert!(waters.len() > 200, "the water is in view: {}", waters.len());
    let (mut edge, mut edge_green, mut away, mut away_green) = (0usize, 0usize, 0usize, 0usize);
    let mut away_red = 0.0f32;
    for i in (0..off.kind.len()).step_by(3) {
        if off.class[i] != 1 || off.kind[i] == WATER_KIND || off.water[i] != 0 {
            continue;
        }
        let [r, g, _] = off.albedo[i];
        if r < g * 4.0 {
            continue;
        }
        let p = stands_at(&off, &inv, i).truncate();
        let from_water = waters
            .iter()
            .map(|w| (w.truncate() - p).length())
            .fold(f32::MAX, f32::min);
        let [r_on, g_on, _] = on.albedo[i];
        if from_water < 5.0 {
            edge += 1;
            edge_green += usize::from(g_on > r_on);
        } else if from_water > 12.0 {
            away += 1;
            away_red += r;
            away_green += usize::from(g_on > r_on);
        }
    }
    eprintln!("mud at the edge: {edge_green} of {edge} green; away: {away_green} of {away} green");
    assert!(
        edge > 100 && away > 1000,
        "the bank is in view: {edge}, {away}"
    );
    assert!(
        away_green * 5 >= away,
        "the grass grows in away from the water: {away_green} of {away}"
    );
    assert!(
        away_green * 10 < away * 9,
        "in patches, not all over: {away_green} of {away}"
    );
    assert!(
        edge_green * away * 10 <= away_green * edge * 7,
        "less of it grows near the edge: {edge_green} of {edge}, {away_green} of {away}"
    );

    // What lay as retail's water picture is the pack's now: pure red and green, nothing blue.
    let (mut water_ground, mut blue) = (0usize, 0usize);
    let (mut film, mut film_red) = (0usize, 0.0f32);
    for i in 0..on.kind.len() {
        if on.class[i] == 1 && on.kind[i] == WATER_KIND {
            let [r, g, b] = on.albedo[i];
            water_ground += 1;
            blue += usize::from(b > r.max(g) * 0.5);
            if on.bed[i] == 0 {
                film += 1;
                film_red += r.max(g);
            }
        }
    }
    eprintln!("the water's ground: {blue} of {water_ground} blue");
    assert!(
        blue * 50 <= water_ground,
        "retail's water ground is laid as the land and the bed: {blue} of {water_ground}"
    );

    // The ground under a film of water is wet, darker than the same materials dry.
    let (dry, wet) = (away_red / away as f32, film_red / film.max(1) as f32);
    eprintln!("dry {dry}, wet {wet}, of {film}");
    assert!(
        film > 100 && wet <= 0.92 * dry,
        "the wet ground is darker: {wet}, dry {dry}"
    );

    let none_on = run_bank(&store, None, 0.7);
    let none_off = run_bank(&store, None, 0.0);
    assert_eq!(
        none_on.color, none_off.color,
        "with no pack the bank is retail's"
    );
}

/// How much of the ground drawn in `c`, red one material and green the other, lies partway between
/// them: the share of its pixels neither mostly one nor mostly the other.
fn partway_share(c: &RemasterCapture) -> f32 {
    let (mut partway, mut ground) = (0usize, 0usize);
    for i in (0..c.class.len()).filter(|i| c.class[*i] == 1) {
        let [r, g, _] = c.albedo[i];
        if r + g < 0.02 {
            continue;
        }
        ground += 1;
        let t = r / (r + g);
        if t > 0.3 && t < 0.7 {
            partway += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let share = partway as f32 / ground.max(1) as f32;
    share
}

/// **A pack's materials meet in a sharper edge close by**: on the shore, where grass grows over the
/// mud in patches, both of one height so only the sharpening moves their edge, little of the ground
/// lies partway between the two.
///
/// Behaviour: remaster.art-pack.remaster-sharpens-blend-edges
#[test]
fn a_packs_materials_meet_in_a_sharper_edge_close_by() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let share = partway_share(&run_bank(&store, Some(bank_pack()), 1.0));
    eprintln!("{share:.4} of the ground lies partway");
    // With the mask's soft edge it comes to 0.087; sharpened, 0.055.
    assert!(share < 0.07, "the edges stay soft: {share}");
}

/// The share a flat channel's spread is of its mean, over `c`'s ground within 100 meters of the
/// eye, in the blocks held.
#[allow(clippy::cast_precision_loss)] // pixels of a frame
fn ground_spread(c: &RemasterCapture) -> f32 {
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let eye = glam::Vec3::from(c.eye);
    let values: Vec<f32> = (0..c.class.len())
        .filter(|i| c.class[*i] == 1 && (stands_at(c, &inv, *i) - eye).length() < 100.0)
        .map(|i| c.albedo[i][1])
        .collect();
    assert!(values.len() > 1000, "the ground is near: {}", values.len());
    let n = values.len() as f32;
    let mean = values.iter().sum::<f32>() / n;
    let var = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
    var.sqrt() / mean.max(1e-3)
}

/// **A pack's ground varies over the land**: in the forest, a pack's perfectly flat material is
/// laid out on the player's ground in the broad patches of the retail picture it replaces and in
/// gentle patches of its own, where the same flat material on retail's grid stays flat.
///
/// Behaviour: remaster.art-pack.remaster-varies-its-ground
#[test]
fn a_packs_flat_material_varies_over_the_land() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let flat = || Some(ground_pack(None, None));
    let plain = run_ground(
        &store,
        &st,
        flat(),
        (
            dereth_scene::world_scene::RemasterTerrain {
                splat: true,
                ..Default::default()
            },
            0,
        ),
    );
    let varied = run_ground(
        &store,
        &st,
        flat(),
        (dereth_scene::world_scene::RemasterTerrain::player(), 0),
    );
    let (a, b) = (ground_spread(&plain), ground_spread(&varied));
    eprintln!("the flat material's spread: on retail's grid {a:.4}, on the player's ground {b:.4}");
    assert!(a < 0.01, "retail's grid keeps it flat: {a}");
    assert!(b > 0.03, "the player's ground varies it: {b}");
}

/// The remaster over the forest with `pack` given, its trees drawn when `drawn`, the sky drawn and
/// the fog as `fog` says, under the knobs `knobs`.
fn over_the_land(
    store: &Arc<RetailDatStore>,
    pack: Option<Arc<ArtPack>>,
    drawn: bool,
    fog: bool,
    knobs: &str,
) -> RemasterCapture {
    let mut gpu = device();
    let mut shot = Shot::open(store, &mut gpu, &over_the_forest());
    shot.poll(&mut gpu);
    shot.scene.draw.set_remaster_pack(pack, None);
    shot.scene.draw.set_remaster_packs(
        if drawn {
            trees()
        } else {
            RemasterPacks::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_sky(
        dereth_scene::world_scene::RemasterSky {
            atmosphere: true,
            fog,
            far_keep: 0.2,
            air: 0.0,
            glow: 0.0,
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_post(
        RemasterPost {
            hdr: true,
            ..RemasterPost::default()
        },
        &mut gpu,
    );
    shot.scene.draw.set_remaster_terrain(
        dereth_scene::world_scene::RemasterTerrain::player(),
        &mut gpu,
    );
    let (parsed, notes) = dereth_terrain::land::drawn::Knobs::parse(knobs);
    assert!(notes.is_empty(), "{notes:?}");
    shot.scene.set_remaster_ground_knobs(parsed);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    shot.scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..5 {
        shot.step_held(&mut gpu);
    }
    let capture = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    capture
}

/// A pixel's brightness in `c`, 0 to 255.
fn luma_at(c: &RemasterCapture, i: usize) -> f32 {
    let [r, g, b] = [0, 1, 2].map(|k| f32::from(c.color[i * 4 + k]));
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// How far apart the land of `c` lies past `from` meters from the eye: the brightest tenth of its
/// pixels' brightness less their darkest tenth, and how many were measured.
fn far_land_spread(c: &RemasterCapture, from: f32) -> (f32, usize) {
    let inv = glam::Mat4::from_cols_array(&c.view_projection).inverse();
    let eye = glam::Vec3::from(c.eye);
    let mut luma: Vec<f32> = (0..c.class.len())
        .filter(|i| c.class[*i] != 0 && (stands_at(c, &inv, *i) - eye).length() > from)
        .map(|i| luma_at(c, i))
        .collect();
    luma.sort_by(f32::total_cmp);
    let n = luma.len();
    if n < 10 {
        return (0.0, n);
    }
    (luma[n * 9 / 10] - luma[n / 10], n)
}

/// **An art pack's air lets far land keep its dark and light**: over the forest, the land past where
/// retail's fog ends keeps its own dark and light through the air, where retail's fog leaves it one
/// flat color.
///
/// Behaviour: remaster.art-pack.remaster-air
#[test]
fn an_art_packs_air_lets_far_land_keep_its_dark_and_light() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = || Some(in_memory(&far_files(true)));
    let (air, pixels) = far_land_spread(&over_the_land(&store, pack(), true, true, ""), 3000.0);
    let (fog, _) = far_land_spread(&over_the_land(&store, pack(), true, false, ""), 3000.0);
    eprintln!(
        "far land spreads {air:.1} through the air, {fog:.1} in retail's fog; {pixels} pixels"
    );
    assert!(pixels > 500, "far land in view: {pixels}");
    // The fog's volume, keeping a share of what lies past its end, comes to 33.1; the air, 48.1.
    assert!(
        air > 40.0 && air > fog * 3.0,
        "{air} through the air, {fog} in the fog"
    );
}

/// **An art pack's clouds shade the ground under them**: over the forest at midday, some of the land
/// is plainly darker than in the same frame with the clouds' shadows off, and next to none of it
/// brighter.
///
/// Behaviour: remaster.art-pack.remaster-cloud-shadows
#[test]
fn an_art_packs_clouds_shade_the_ground_under_them() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = || Some(in_memory(&far_files(true)));
    let shaded = over_the_land(&store, pack(), true, true, "");
    let open = over_the_land(&store, pack(), true, true, "cloud_shadows = 0");
    let land: Vec<usize> = (0..shaded.class.len())
        .filter(|i| shaded.class[*i] != 0)
        .collect();
    let mut shade: Vec<f32> = land
        .iter()
        .map(|i| luma_at(&shaded, *i) / luma_at(&open, *i).max(1.0))
        .collect();
    shade.sort_by(f32::total_cmp);
    let n = shade.len();
    let patches = shade[n * 9 / 10] - shade[n / 10];
    let darker = land
        .iter()
        .filter(|i| luma_at(&open, **i) - luma_at(&shaded, **i) > 3.0)
        .count();
    let brighter = land
        .iter()
        .filter(|i| luma_at(&shaded, **i) - luma_at(&open, **i) > 3.0)
        .count();
    eprintln!(
        "{darker} of {} pixels darker in shadow, {brighter} brighter, the shade varying {patches:.3}",
        land.len()
    );
    // Lying in patches, not as one dimming of the frame: 0.124.
    assert!(patches > 0.05, "the shade is even: {patches}");
    assert!(
        darker > land.len() / 50,
        "no shadow: {darker} of {}",
        land.len()
    );
    assert!(
        brighter < darker / 10,
        "{brighter} brighter, {darker} darker"
    );
}

/// **With nothing of a pack drawn the land keeps the fog**: a pack given and nothing of it drawn
/// leaves the frame the one with no pack at all, sky and fog on: byte for byte where two frames
/// with no pack are the same byte for byte (Vulkan and Direct3D 12 on Windows), and within a few
/// times what two frames with no pack differ by where they are not (MoltenVK).
///
/// Behaviour: remaster.art-pack.remaster-air
#[test]
fn a_pack_with_nothing_drawn_leaves_the_fog_as_it_was() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let given = over_the_land(&store, Some(in_memory(&far_files(true))), false, true, "");
    let none = over_the_land(&store, None, false, true, "");
    let again = over_the_land(&store, None, false, true, "");
    // What the backend's own frames differ by, drawn twice the same way.
    let noise = differ(&none, &again);
    let bound = if noise == 0 { 0 } else { (3 * noise).max(32) };
    let changed = differ(&given, &none);
    assert!(
        changed <= bound,
        "the frames differ at {changed} pixels; two frames with no pack differ at {noise}"
    );
}

/// A flat, one-sided, opaque magenta square `side` across at `height` over the model's foot, as
/// glTF: its face up, its triangles counter-clockwise seen from above as glTF's front faces are,
/// or the other way round when `backwards`.
fn flat_square(side: f32, height: f32, backwards: bool) -> (String, Vec<u8>) {
    let s = side / 2.0;
    let pos = [
        [-s, height, -s],
        [s, height, -s],
        [s, height, s],
        [-s, height, s],
    ];
    let mut bin = Vec::new();
    for p in pos {
        for c in p {
            bin.extend(c.to_le_bytes());
        }
    }
    for _ in 0..4 {
        for c in [0.0f32, 1.0, 0.0] {
            bin.extend(c.to_le_bytes());
        }
    }
    let indices: [u32; 6] = if backwards {
        [0, 1, 2, 0, 2, 3]
    } else {
        [0, 2, 1, 0, 3, 2]
    };
    for i in indices {
        bin.extend(i.to_le_bytes());
    }
    let gltf = format!(
        r#"{{"asset": {{"version": "2.0"}}, "scenes": [{{"nodes": [0]}}], "nodes": [{{"mesh": 0}}],
        "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0, "NORMAL": 1}},
            "indices": 2, "material": 0}}]}}],
        "materials": [{{"pbrMetallicRoughness": {{"baseColorFactor": [1.0, 0.0, 1.0, 1.0],
            "metallicFactor": 0.0}}}}],
        "buffers": [{{"uri": "square.bin", "byteLength": {len}}}],
        "bufferViews": [{{"buffer": 0, "byteOffset": 0, "byteLength": 48}},
            {{"buffer": 0, "byteOffset": 48, "byteLength": 48}},
            {{"buffer": 0, "byteOffset": 96, "byteLength": 24}}],
        "accessors": [{{"bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3",
                "min": [-{s}, {height}, -{s}], "max": [{s}, {height}, {s}]}},
            {{"bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC3"}},
            {{"bufferView": 2, "componentType": 5125, "count": 6, "type": "SCALAR"}}]}}"#,
        len = bin.len()
    );
    (gltf, bin)
}

/// A pack whose most common tree is a [`flat_square`] ten metres across, fifteen up.
fn square_pack(backwards: bool) -> Arc<ArtPack> {
    let (gltf, bin) = flat_square(10.0, 15.0, backwards);
    let mut files = HashMap::new();
    files.insert(
        "manifest.toml".to_owned(),
        format!(
            "[pack]\nname = \"Squares\"\nformat = 1\n[[model]]\nid = \"{TREE}\"\ncategory = \
             \"tree\"\nauthor = \"a\"\ntool = \"t\"\nlicense = \"CC0-1.0\"\n[[model.lod]]\nfile \
             = \"square.gltf\"\n"
        )
        .into_bytes(),
    );
    files.insert("square.gltf".to_owned(), gltf.into_bytes());
    files.insert("square.bin".to_owned(), bin);
    in_memory(&files)
}

/// The forest seen from straight above.
fn above_the_forest() -> Station {
    Station {
        name: "above-the-forest",
        why: "the forest's trees from straight above",
        place: Place::Free {
            block: 0x48C6,
            x: 96.0,
            y: 96.0,
            eye: 80.0,
            yaw: 0.0,
            pitch: -1.5,
        },
        clock: Clock::Sunny(0.5),
        weather: false,
        land_radius: 2,
    }
}

/// **A one-sided model's face is the one its glTF triangles turn counter-clockwise on**: a flat,
/// one-sided square in place of each tree, its face up as glTF winds it, is drawn seen from
/// above; the same square wound the other way round is its back from above, and draws nothing.
#[test]
fn a_one_sided_pack_models_gltf_front_face_is_the_one_drawn() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = above_the_forest();
    let up = run(&store, &st, Some((square_pack(false), None)), trees(), 6);
    let backwards = run(&store, &st, Some((square_pack(true), None)), trees(), 6);
    let (seen, unseen) = (magenta(&up.capture), magenta(&backwards.capture));
    eprintln!(
        "{} squares placed: {seen} magenta pixels face up, {unseen} wound the other way",
        up.counts.placements
    );
    assert!(up.counts.placements > 10, "{:?}", up.counts);
    assert!(seen > 2_000, "the squares' faces are drawn: {seen} pixels");
    assert!(
        unseen * 20 < seen,
        "their backs are not: {unseen} pixels against {seen}"
    );
}

/// The pack of cards with a red leaf in place of the green, and its `lighting` section.
fn red_leaf_files(lighting: Option<&str>) -> HashMap<String, Vec<u8>> {
    let side = 16u32;
    let mut rgba = Vec::new();
    for y in 0..side {
        for x in 0..side {
            #[allow(clippy::cast_precision_loss)] // a small picture's texels
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            rgba.extend(if dx * dx + dy * dy < 6.5 * 6.5 {
                [190, 50, 35, 255]
            } else {
                [255, 0, 255, 0]
            });
        }
    }
    let red = png(side, side, &rgba);
    let mut files = files(lighting);
    for level in ["near", "far"] {
        files.insert(format!("{level}/leaf.png"), red.clone());
    }
    files
}

/// Whether the drawn pixel `i` is green, or red: its hue within 60 degrees of the one named, and
/// saturated enough for its hue to show.
fn hue_near(c: &RemasterCapture, i: usize, degrees: f32) -> bool {
    let [r, g, b] = [0, 1, 2].map(|k| f32::from(c.color[i * 4 + k]));
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    if hi < 20.0 || (hi - lo) / hi < 0.25 {
        return false;
    }
    let span = hi - lo;
    let h = if hi == r {
        ((g - b) / span).rem_euclid(6.0)
    } else if hi == g {
        2.0 + (b - r) / span
    } else {
        4.0 + (r - g) / span
    } * 60.0;
    let off = (h - degrees + 180.0).rem_euclid(360.0) - 180.0;
    off.abs() < 60.0
}

/// **An art pack's leaves are pulled toward one green, and nothing else is**: in the forest, a
/// pack's trees with red leaves are drawn green; with the pack's own lighting saying to pull them
/// not at all, they are drawn red; and between the two frames no pixel changes but the leaves'.
#[test]
fn a_packs_leaves_are_pulled_toward_one_green_and_nothing_else_is() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let st = station(&store, "forest");
    let pulled = run(
        &store,
        &st,
        Some((in_memory(&red_leaf_files(None)), None)),
        trees(),
        6,
    );
    let as_made = run(
        &store,
        &st,
        Some((in_memory(&red_leaf_files(Some("foliage_pull = 0.0"))), None)),
        trees(),
        6,
    );
    // The leaves are what changes between the two frames.
    let changed: Vec<usize> = (0..pulled.capture.class.len())
        .filter(|&i| {
            pulled.capture.color[i * 4..i * 4 + 3] != as_made.capture.color[i * 4..i * 4 + 3]
        })
        .collect();
    let red = changed
        .iter()
        .filter(|&&i| hue_near(&as_made.capture, i, 10.0))
        .count();
    let green = changed
        .iter()
        .filter(|&&i| hue_near(&pulled.capture, i, 100.0))
        .count();
    eprintln!(
        "{} pixels changed: {red} of them red as made, {green} green pulled",
        changed.len()
    );
    assert!(
        changed.len() > 2_000,
        "the leaves are drawn: {}",
        changed.len()
    );
    assert!(
        red * 50 >= changed.len() * 49,
        "nothing but the red leaves changes: {red} of {} changed pixels",
        changed.len()
    );
    assert!(
        green * 10 >= changed.len() * 9,
        "pulled, they are green: {green} of {} changed pixels",
        changed.len()
    );
}
