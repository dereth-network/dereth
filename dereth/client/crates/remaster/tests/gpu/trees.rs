//! Retail's trees chosen on the device against the processor's own choice of the same scene: each
//! tree's level, the level it fades into and how far, and how much of it its impostor draws, by
//! retail's bands and its walk's levels, past the cap the far forest sets; the same cards for
//! every tree in view; and the same frame, to the byte, though the device makes the trees'
//! pictures in another order.
//!
//! Behaviour: none (experimental)

use dereth_gfx::Format;
use dereth_remaster::assets::pack::PackModel;
use dereth_remaster::assets::{
    Appearance, AssetSource, BlendFactor, CellPieceKey, Kind, Material, MaterialKey, Mesh,
    SubsetInfo, SurfaceRef, TerrainCell, Texture, TextureFormat, TextureKey, Use, Vertex,
};
use dereth_remaster::render::impostor::DrawnCard;
use dereth_remaster::render::instances::seen;
use dereth_remaster::render::light::LightMode;
use dereth_remaster::render::lod::{retail_switches, LodSettings, PartDraw};
use dereth_remaster::render::{ReadBack, Renderer};
use dereth_remaster::scene::{
    Block, BlockId, BlockLevels, Camera, Degrade, DegradeBias, FrameDelta, Part, PartLook,
    Placement, PlacementKind, RetailLight, SceneDelta, Terrain,
};
use glam::{Mat4, Vec3};

const BLOCK: BlockId = 0xA9B4;
const SIZE: (u32, u32) = (640, 360);
/// A tree's full mesh, its coarser level 0.3 m off it, and past them nothing.
const FULL: u32 = 0x0100_0010;
const COARSE: u32 = 0x0100_0011;
/// The nearest trees, each with levels of its own: the same squares as other meshes, from
/// `NEAR_MESHES` on, two a tree. The device makes their pictures as the block comes, before the far
/// trees', while the processor never makes them, so the far trees' pictures stand in a row of the
/// atlas well down its page on the device and in its first on the processor.
const NEAR: u32 = 5;
const NEAR_MESHES: u32 = 0x0100_0020;
/// The trees' picture, and its side in texels.
const PICTURE: u32 = 0x0600_0001;
const PICTURE_SIDE: u32 = 256;
/// Where the eye stands, block-local, looking north.
const EYE: Vec3 = Vec3::new(96.0, 4.0, 3.0);
/// The trees: how far north of the eye, how far east of its line, and their scale.
const TREES: &[(f32, f32, f32)] = &[
    (6.0, 1.0, 1.0),
    (15.0, -2.0, 1.0),
    (30.0, 3.0, 0.8),
    (45.0, -3.0, 1.0),
    (80.0, -4.0, 1.0),
    (101.0, 5.0, 1.0),
    // Fading into the second level, a scaled one too.
    (115.0, -5.0, 1.0),
    (120.0, 6.0, 1.0),
    (150.0, -6.0, 1.3),
    (160.0, 7.0, 1.0),
    // Fading into its impostor, then wholly it.
    (180.0, -7.0, 1.0),
    (190.0, 8.0, 1.0),
    (200.0, -8.0, 1.0),
    (240.0, 9.0, 1.0),
    // Past the cap the far forest sets.
    (230.0, -9.0, 1.3),
    (300.0, 10.0, 1.0),
    (380.0, -10.0, 1.1),
    // Behind the eye, and far off to the side.
    (-30.0, 0.0, 1.0),
    (20.0, 150.0, 1.0),
];
/// Retail's bands for every tree's levels.
const BANDS: [[f32; 3]; 3] = [
    [20.0, 40.0, 60.0],
    [60.0, 90.0, 120.0],
    [f32::MAX, f32::MAX, f32::MAX],
];

/// A tree's level: a square eight meters across standing upright, `off` meters east of its
/// place, of surface 8.
fn upright(off: f32) -> Mesh {
    let v = |x: f32, z: f32| Vertex {
        position: [x - 4.0 + off, 0.0, z],
        normal: [0.0, -1.0, 0.0],
        uv: [x / 8.0, z / 8.0],
    };
    let mut m = Mesh::from_triangle_lists([(
        SubsetInfo {
            surface: SurfaceRef::Surface(8),
            two_sided: true,
            tiled: false,
        },
        vec![
            v(0.0, 0.0),
            v(8.0, 0.0),
            v(8.0, 8.0),
            v(0.0, 0.0),
            v(8.0, 8.0),
            v(0.0, 8.0),
        ],
    )]);
    m.sphere = Some(([off, 0.0, 4.0], 4.0 * std::f32::consts::SQRT_2));
    m
}

/// The trees' meshes, all foliage.
struct Source;

impl AssetSource for Source {
    fn gfxobj(&mut self, id: u32) -> Option<Mesh> {
        let near = (NEAR_MESHES..NEAR_MESHES + 2 * NEAR).contains(&id);
        match id {
            FULL => Some(upright(0.0)),
            COARSE => Some(upright(0.3)),
            _ if near => Some(upright(if (id - NEAR_MESHES).is_multiple_of(2) {
                0.0
            } else {
                0.3
            })),
            _ => None,
        }
    }

    fn cell_piece(&mut self, _key: CellPieceKey) -> Option<Mesh> {
        None
    }

    fn appearance(&mut self, _surface: u32, _look: Option<&PartLook>) -> Appearance {
        Appearance::default()
    }

    fn material(&mut self, _key: &MaterialKey, _used: Use) -> Material {
        Material {
            kind: Kind::Foliage,
            surface_type: 2,
            data_category: Some(1),
            texture: Some(TextureKey::Dat {
                picture: PICTURE,
                palette: 0,
            }),
            clip_map: false,
            alpha_test: Some(128),
            vertex_alpha: 255,
            vertex_colored: false,
            translucent: None,
            additive: false,
            blended: false,
            src_blend: BlendFactor::One,
            dst_blend: BlendFactor::Zero,
            z_write: true,
            fogged: true,
            solid: None,
            luminosity: 0.0,
            diffuse: 1.0,
            maps: None,
        }
    }

    /// A picture of every texel its own color, a third of them cut away, so the trees' pictures
    /// show where each is read, and their crowns are thin in places as leaves are.
    fn texture(&mut self, _key: &TextureKey, _clip_map: bool) -> Option<Texture> {
        let side = PICTURE_SIDE;
        let texel = |x: u32, y: u32| {
            let h = (x * 73 + y * 151) ^ (x * y * 29);
            let byte = |k: u32| (h * k).to_le_bytes()[0];
            [
                byte(37),
                byte(91),
                byte(13),
                if h.is_multiple_of(3) { 0 } else { 255 },
            ]
        };
        Some(Texture {
            width: side,
            height: side,
            format: TextureFormat::Bgra8,
            levels: vec![(0..side * side)
                .flat_map(|i| texel(i % side, i / side))
                .collect()],
        })
    }

    fn terrain_cells(&mut self, _block: BlockId, _t: &Terrain) -> Vec<TerrainCell> {
        Vec::new()
    }

    fn sky_objects(&mut self) -> Vec<u32> {
        Vec::new()
    }

    fn sky(&mut self, _id: u32) -> Option<Mesh> {
        None
    }

    fn pack_model(&mut self, _id: u32) -> Option<PackModel> {
        None
    }
}

/// Where tree `k` stands, block-local.
fn at(k: usize) -> Vec3 {
    let (north, east, _) = TREES[k];
    Vec3::new(EYE.x + east, EYE.y + north, 0.0)
}

/// The level retail's walk draws tree `k` at: from its distance at an even bias, but its first for
/// every third tree, as a walk measuring another distance can, which keeps it from coarser levels.
fn walked_level(k: usize) -> u8 {
    if k % 3 == 2 {
        return 0;
    }
    let record = degrade(k);
    let d = record.centre.distance(EYE);
    let switches = retail_switches(&record, &DegradeBias::default());
    switches.iter().filter(|s| d >= **s).count() as u8
}

/// Tree `k`'s first two levels' meshes.
fn meshes(k: usize) -> (u32, u32) {
    match u32::try_from(k) {
        Ok(k) if k < NEAR => (NEAR_MESHES + 2 * k, NEAR_MESHES + 2 * k + 1),
        _ => (FULL, COARSE),
    }
}

/// Tree `k`'s degrade record.
fn degrade(k: usize) -> Degrade {
    let scale = TREES[k].2;
    let (full, coarse) = meshes(k);
    Degrade {
        levels: vec![full, coarse, 0],
        billboards: false,
        bands: BANDS.to_vec(),
        modes: vec![1; 3],
        scale_z: scale,
        centre: at(k) + Vec3::new(0.0, 0.0, 4.0 * scale),
    }
}

/// The block of the trees.
fn block() -> Block {
    let mut vertices = Vec::new();
    for i in 0..9u8 {
        for j in 0..9u8 {
            vertices.push(Vec3::new(f32::from(i) * 24.0, f32::from(j) * 24.0, 0.0));
        }
    }
    let placements = (0..TREES.len())
        .map(|k| {
            let frame = Mat4::from_translation(at(k));
            let scale = TREES[k].2;
            Placement {
                id: 0x0200_0001,
                kind: PlacementKind::Object,
                frame,
                scale,
                lifts: [0.0; 2],
                parts: vec![Part {
                    gfxobj: meshes(k).0,
                    frame,
                    scale: Vec3::splat(scale),
                    degrade: Some(u32::try_from(k).unwrap()),
                }],
            }
        })
        .collect();
    Block {
        block: BLOCK,
        generation: 1,
        terrain: Terrain {
            side: 8,
            vertices,
            cuts: vec![false; 64],
            words: [4; 81],
        },
        placements,
        degrades: (0..TREES.len()).map(degrade).collect(),
        rooms: Vec::new(),
        lights: Vec::new(),
        lamps: Vec::new(),
    }
}

fn frame() -> FrameDelta {
    let forward = Vec3::new(0.0, 1.0, -0.02).normalize();
    let aspect = SIZE.0 as f32 / SIZE.1 as f32;
    FrameDelta {
        origin: [0xA9, 0xB4],
        landscape: true,
        drawn_blocks: vec![BLOCK],
        // The walk chose levels for all but the last two trees.
        levels: vec![BlockLevels {
            block: BLOCK,
            levels: (0..TREES.len() - 2).map(walked_level).collect(),
            turned: Vec::new(),
        }],
        camera: Camera {
            view: Mat4::look_to_lh(EYE, forward, Vec3::Z),
            projection: Mat4::perspective_lh(1.0, aspect, 0.3, 2000.0),
            previous_view_projection: None,
            eye: EYE,
            viewport: [0, 0, SIZE.0, SIZE.1],
            fov_y: 1.0,
            aspect,
            near: 0.3,
            far: 2000.0,
        },
        light: RetailLight {
            sun: Vec3::new(0.3, -0.2, 0.9),
            sun_color: [255; 3],
            ambient_level: 0.3,
            ambient_color: [255; 3],
            object_ambient: [0.3; 3],
            outdoor: true,
            sees_outside: true,
            time_of_day: 0.5,
            ..RetailLight::default()
        },
        // The far forest stands the trees past 280 m.
        scenery_reach: 280.0,
        ..FrameDelta::default()
    }
}

/// A renderer that has drawn the trees for four frames, their levels chosen on the device or on
/// the processor.
fn drawn(on_device: bool) -> Renderer {
    let mut r = Renderer::new(crate::hardware(), Format::D32Float).expect("a renderer");
    r.set_mode(LightMode::Pixel);
    r.set_lod(LodSettings::ON);
    r.set_parts_on_device(on_device);
    r.receive(
        vec![SceneDelta::BlockIn(Box::new(block()))],
        frame(),
        [None; 2],
        &mut Source,
    );
    for _ in 0..4 {
        assert!(r.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
        r.receive(Vec::new(), frame(), [None; 2], &mut Source);
    }
    assert!(r.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
    r
}

/// A card's place, size and share, to the bit.
fn key(c: &DrawnCard) -> (u32, [u32; 3], u32, u32) {
    (
        c.gfxobj,
        c.centre.to_array().map(f32::to_bits),
        c.radius.to_bits(),
        c.share.to_bits(),
    )
}

/// How many bytes of each of a frame's pictures differ from another's.
fn differing(a: &ReadBack, b: &ReadBack) -> Vec<(&'static str, usize)> {
    let count = |x: &[u8], y: &[u8]| {
        x.iter().zip(y).filter(|(p, q)| p != q).count() + x.len().abs_diff(y.len())
    };
    vec![
        ("color", count(&a.color, &b.color)),
        ("albedo", count(&a.albedo, &b.albedo)),
        ("normals", count(&a.normal, &b.normal)),
        ("depth", count(&a.depth, &b.depth)),
        ("materials", count(&a.material, &b.material)),
    ]
}

#[test]
fn retails_trees_on_the_device_draw_the_processors_levels_fades_impostors_cards_and_frame() {
    let mut cpu = drawn(false);
    assert_eq!(cpu.parts_on_device(), 0);
    let choices: Vec<PartDraw> = cpu.part_choices(BLOCK);
    let f = frame();
    let clip = f.camera.projection * f.camera.view;
    let mut cpu_cards: Vec<_> = cpu
        .impostor_cards()
        .iter()
        .filter(|c| seen(clip, c.centre, c.radius))
        .map(key)
        .collect();
    let cpu_frame = cpu.read_back().expect("the processor's frame");
    let cpu_trees = cpu.impostor_stats().trees;
    drop(cpu);

    let mut gpu = drawn(true);
    assert_eq!(gpu.parts_on_device(), TREES.len());
    let states = gpu.part_states();
    assert_eq!(states.len(), TREES.len());
    let (mut fading, mut impostors, mut into_impostor, mut levels) = (0, 0, 0, [0; 4]);
    for s in &states {
        let want = choices[s.part as usize];
        let (next, fade) = want.fade.unwrap_or((want.level, 0.0));
        let tree = format!("tree {} {:?}", s.part, TREES[s.part as usize]);
        assert_eq!(s.level, want.level, "{tree}: {s:?} against {want:?}");
        assert_eq!(s.next, next, "{tree}: {s:?} against {want:?}");
        assert_eq!(
            s.fade.to_bits(),
            fade.to_bits(),
            "{tree}: {s:?} against {want:?}"
        );
        assert_eq!(
            s.share.to_bits(),
            want.impostor.to_bits(),
            "{tree}: {s:?} against {want:?}"
        );
        fading += usize::from(fade > 0.0);
        impostors += usize::from(want.impostor >= 1.0);
        into_impostor += usize::from(want.impostor > 0.0 && want.impostor < 1.0);
        levels[usize::from(want.level.min(3))] += 1;
    }
    // The scene reaches every case: trees on each level, fading, wholly impostors and fading
    // into them.
    assert!(levels.iter().take(3).all(|n| *n > 0), "levels {levels:?}");
    assert!(fading >= 2, "{fading} trees fade");
    assert!(impostors >= 2, "{impostors} trees are impostors");
    assert!(
        into_impostor >= 1,
        "{into_impostor} trees fade into impostors"
    );
    let mut gpu_cards: Vec<_> = gpu.impostor_cards().iter().map(key).collect();
    cpu_cards.sort_unstable();
    gpu_cards.sort_unstable();
    assert!(!gpu_cards.is_empty());
    assert_eq!(gpu_cards, cpu_cards, "the same cards in view");
    // The device made the nearest trees' pictures too, first, so the far trees' stand in another
    // row than the processor's.
    assert_eq!(gpu.impostor_stats().trees, cpu_trees + NEAR);
    let gpu_frame = gpu.read_back().expect("the device's frame");
    let differ = differing(&cpu_frame, &gpu_frame);
    assert!(
        differ.iter().all(|(_, n)| *n == 0),
        "bytes differing: {differ:?}"
    );
    drop(gpu);
    crate::no_validation_errors();
}
