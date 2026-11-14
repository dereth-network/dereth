//! Blocks' parts with cut-out edges, blended surfaces and added ones, chosen and listed on the
//! device, against the processor's lists of the same scene: the edges' and the blended surfaces'
//! lists hold the same items in the same order, farthest first and by item where two stand as
//! far, with the processor's own turning parts among the device's; the G-buffer's buckets hold
//! the device's parts and the processor's in the walk's order, block by block as the frame drew
//! them; and the frame is the same, to the byte, though the device draws each list with one
//! pipeline for both sides and, for the blended surfaces, both blends.
//!
//! Behaviour: none (experimental)

use std::collections::BTreeSet;

use dereth_gfx::Format;
use dereth_remaster::assets::pack::PackModel;
use dereth_remaster::assets::{
    Appearance, AssetSource, BlendFactor, CellPieceKey, Kind, Material, MaterialKey, Mesh,
    SubsetInfo, SurfaceRef, TerrainCell, Texture, TextureFormat, TextureKey, Use, Vertex,
};
use dereth_remaster::render::light::LightMode;
use dereth_remaster::render::lod::LodSettings;
use dereth_remaster::render::{ReadBack, Renderer};
use dereth_remaster::scene::{
    Block, BlockId, BlockLevels, Camera, Degrade, FrameDelta, Part, PartLook, Placement,
    PlacementKind, RetailLight, SceneDelta, Terrain,
};
use glam::{Mat4, Vec3};

/// The eye's block, and the one north of it, which comes second but is drawn first.
const NEAR_BLOCK: BlockId = 0xA9B4;
const FAR_BLOCK: BlockId = 0xA9B5;
const SIZE: (u32, u32) = (640, 360);
/// Where the eye stands, in the near block, looking north.
const EYE: Vec3 = Vec3::new(96.0, 4.0, 1.5);

/// The surfaces: opaque, a cut-out whose partly transparent texels are blended, glass blended
/// over what is behind, and a glow added to it.
const OPAQUE: u32 = 7;
const CUT_OUT: u32 = 8;
const GLASS: u32 = 9;
const GLOW: u32 = 10;

/// The models: a square of one surface each, one-sided where noted (turned away from the eye
/// where noted, so the face it shows is culled); and a square of opaque with a cut-out before it.
const CUT: u32 = 0x0100_0001;
const CUT_ONE_SIDED: u32 = 0x0100_0002;
const PANE: u32 = 0x0100_0003;
const PANE_ONE_SIDED: u32 = 0x0100_0004;
const LAMP: u32 = 0x0100_0005;
const WALL: u32 = 0x0100_0006;
const CUT_AWAY: u32 = 0x0100_0007;
const PANE_AWAY: u32 = 0x0100_0008;

/// What stands in the near block: the model, how far north of the eye and east of its line,
/// whether it turns toward the viewer (which the processor chooses), and whether the eye sees it.
const NEAR_PLACED: &[(u32, f32, f32, bool, bool)] = &[
    (CUT, 6.0, 0.4, false, true),
    (PANE, 6.5, -0.2, false, true),
    (LAMP, 5.5, 0.0, false, true),
    (WALL, 9.0, 0.0, false, true),
    // As far as each other: their keys tie, and the item decides.
    (CUT, 14.0, 1.5, false, true),
    (CUT_ONE_SIDED, 14.0, -1.5, false, true),
    (PANE, 20.0, 2.0, false, true),
    (PANE_ONE_SIDED, 20.0, -2.0, false, true),
    (LAMP, 18.0, 0.5, false, true),
    // Turning toward the viewer: the processor's, among the device's.
    (WALL, 12.0, -0.8, true, true),
    (CUT, 30.0, 0.0, true, true),
    (PANE, 40.0, -3.0, false, true),
    // One-sided and turned away, before the walls and panes behind them.
    (CUT_AWAY, 8.0, 0.3, false, true),
    (PANE_AWAY, 8.5, -0.3, false, true),
    (CUT, 55.0, 4.0, false, true),
    (LAMP, 70.0, -4.0, false, true),
    (WALL, 90.0, 1.0, false, true),
    // Behind the eye.
    (CUT, -10.0, 0.0, false, false),
    (PANE, -12.0, 0.5, false, false),
];

/// What stands in the far block, as in the near one.
const FAR_PLACED: &[(u32, f32, f32, bool, bool)] = &[
    (CUT, 200.0, 3.0, false, true),
    (PANE, 205.0, -3.0, false, true),
    (WALL, 210.0, 0.0, false, true),
    (LAMP, 198.0, 1.0, true, true),
];

/// A square two meters across standing upright, of surface `surface`, `ahead` meters north of its
/// place: facing south, or turned away from the south where `away`.
fn upright(surface: u32, two_sided: bool, ahead: f32) -> (SubsetInfo, Vec<Vertex>) {
    facing(surface, two_sided, ahead, false)
}

fn facing(surface: u32, two_sided: bool, ahead: f32, away: bool) -> (SubsetInfo, Vec<Vertex>) {
    let v = |x: f32, z: f32| Vertex {
        position: [x - 1.0, ahead, z],
        normal: [0.0, -1.0, 0.0],
        uv: [x / 2.0, z / 2.0],
    };
    // Wound so that, seen from the south, it is the face a culled pipeline keeps, or, turned
    // away, the face it culls.
    let mut corners = vec![
        v(0.0, 0.0),
        v(2.0, 2.0),
        v(2.0, 0.0),
        v(0.0, 0.0),
        v(0.0, 2.0),
        v(2.0, 2.0),
    ];
    if away {
        corners.swap(1, 2);
        corners.swap(4, 5);
    }
    (
        SubsetInfo {
            surface: SurfaceRef::Surface(surface),
            two_sided,
            tiled: false,
        },
        corners,
    )
}

struct Source;

impl AssetSource for Source {
    fn gfxobj(&mut self, id: u32) -> Option<Mesh> {
        let lists = match id {
            CUT => vec![upright(CUT_OUT, true, 0.0)],
            CUT_ONE_SIDED => vec![upright(CUT_OUT, false, 0.0)],
            PANE => vec![upright(GLASS, true, 0.0)],
            PANE_ONE_SIDED => vec![upright(GLASS, false, 0.0)],
            LAMP => vec![upright(GLOW, true, 0.0)],
            WALL => vec![upright(OPAQUE, true, 0.3), upright(CUT_OUT, true, 0.0)],
            CUT_AWAY => vec![facing(CUT_OUT, false, 0.0, true)],
            PANE_AWAY => vec![facing(GLASS, false, 0.0, true)],
            _ => return None,
        };
        Some(Mesh::from_triangle_lists(lists))
    }

    fn cell_piece(&mut self, _key: CellPieceKey) -> Option<Mesh> {
        None
    }

    fn appearance(&mut self, _surface: u32, _look: Option<&PartLook>) -> Appearance {
        Appearance::default()
    }

    fn material(&mut self, key: &MaterialKey, _used: Use) -> Material {
        let (alpha_test, blended, dst_blend, translucent) = match key.surface {
            CUT_OUT => (Some(100), true, BlendFactor::InvSrcAlpha, None),
            GLASS => (None, true, BlendFactor::InvSrcAlpha, Some(140)),
            GLOW => (None, true, BlendFactor::One, None),
            _ => (None, false, BlendFactor::Zero, None),
        };
        Material {
            kind: Kind::Item,
            surface_type: 2,
            data_category: Some(1),
            texture: Some(TextureKey::Dat {
                picture: 0x0600_0000 + key.surface,
                palette: 0,
            }),
            clip_map: false,
            alpha_test,
            vertex_alpha: translucent.map_or(255, |t| 255 - t),
            vertex_colored: false,
            translucent,
            additive: dst_blend == BlendFactor::One,
            blended,
            src_blend: if blended {
                BlendFactor::SrcAlpha
            } else {
                BlendFactor::One
            },
            dst_blend,
            z_write: !blended || alpha_test.is_some(),
            fogged: dst_blend != BlendFactor::One,
            solid: None,
            luminosity: 0.0,
            diffuse: 1.0,
            maps: None,
        }
    }

    /// A picture of every texel its own color, its alpha by turns cut away, partly transparent at
    /// two strengths, and solid, so a cut-out's edges and its solid texels both show.
    fn texture(&mut self, key: &TextureKey, _clip_map: bool) -> Option<Texture> {
        let TextureKey::Dat { picture, .. } = key else {
            return None;
        };
        let side = 64u32;
        let seed = picture & 0xFF;
        let texel = |x: u32, y: u32| {
            let h = (x * 73 + y * 151 + seed * 17) ^ (x * y * 29);
            let byte = |k: u32| (h * k).to_le_bytes()[0];
            let alpha = [0, 150, 220, 255][(h % 4) as usize];
            [byte(37), byte(91), byte(13), alpha]
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

/// Block `id` of the objects `placed`, the eye's foot `foot` meters off its south-west corner.
fn block_of(id: BlockId, foot: Vec3, placed: &[(u32, f32, f32, bool, bool)]) -> Block {
    let mut vertices = Vec::new();
    for i in 0..9u8 {
        for j in 0..9u8 {
            vertices.push(Vec3::new(f32::from(i) * 24.0, f32::from(j) * 24.0, 0.0));
        }
    }
    let mut degrades = Vec::new();
    let placements = placed
        .iter()
        .map(|&(model, north, east, turns, _)| {
            let frame = Mat4::from_translation(foot + Vec3::new(east, north, 0.0));
            // A part that turns degrades by a record whose one level turns toward the viewer.
            let degrade = turns.then(|| {
                degrades.push(Degrade {
                    levels: vec![model],
                    billboards: false,
                    bands: vec![[f32::MAX; 3]],
                    modes: vec![2],
                    scale_z: 1.0,
                    centre: frame.w_axis.truncate() + Vec3::new(0.0, 0.0, 1.0),
                });
                u32::try_from(degrades.len() - 1).unwrap()
            });
            Placement {
                id: 0x0200_0000 + model,
                kind: PlacementKind::Object,
                frame,
                scale: 1.0,
                lifts: [0.0; 2],
                parts: vec![Part {
                    gfxobj: model,
                    frame,
                    scale: Vec3::ONE,
                    degrade,
                }],
            }
        })
        .collect();
    Block {
        block: id,
        generation: 1,
        terrain: Terrain {
            side: 8,
            vertices,
            cuts: vec![false; 64],
            words: [4; 81],
        },
        placements,
        degrades,
        rooms: Vec::new(),
        lights: Vec::new(),
        lamps: Vec::new(),
    }
}

/// The two blocks, the near one first.
fn blocks() -> Vec<SceneDelta> {
    let foot = Vec3::new(EYE.x, EYE.y, 0.0);
    vec![
        SceneDelta::BlockIn(Box::new(block_of(NEAR_BLOCK, foot, NEAR_PLACED))),
        SceneDelta::BlockIn(Box::new(block_of(
            FAR_BLOCK,
            foot - Vec3::new(0.0, 192.0, 0.0),
            FAR_PLACED,
        ))),
    ]
}

fn frame() -> FrameDelta {
    let forward = Vec3::new(0.0, 1.0, -0.01).normalize();
    let aspect = SIZE.0 as f32 / SIZE.1 as f32;
    let levels = |id, placed: &[(u32, f32, f32, bool, bool)]| BlockLevels {
        block: id,
        levels: vec![0; placed.iter().filter(|p| p.3).count()],
        turned: Vec::new(),
    };
    FrameDelta {
        origin: [0xA9, 0xB4],
        landscape: true,
        // The far block first: the walk's order, not the order the blocks came in.
        drawn_blocks: vec![FAR_BLOCK, NEAR_BLOCK],
        levels: vec![
            levels(NEAR_BLOCK, NEAR_PLACED),
            levels(FAR_BLOCK, FAR_PLACED),
        ],
        camera: Camera {
            view: Mat4::look_to_lh(EYE, forward, Vec3::Z),
            projection: Mat4::perspective_lh(1.0, aspect, 0.3, 1000.0),
            previous_view_projection: None,
            eye: EYE,
            viewport: [0, 0, SIZE.0, SIZE.1],
            fov_y: 1.0,
            aspect,
            near: 0.3,
            far: 1000.0,
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
        ..FrameDelta::default()
    }
}

/// A renderer that has drawn the blocks for three frames, their parts chosen on the device or on
/// the processor.
fn drawn(on_device: bool) -> Renderer {
    let mut r = Renderer::new(crate::hardware(), Format::D32Float).expect("a renderer");
    r.set_mode(LightMode::Pixel);
    r.set_lod(LodSettings::ON);
    r.set_parts_on_device(on_device);
    r.receive(blocks(), frame(), [None; 2], &mut Source);
    assert!(r.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
    for _ in 1..3 {
        r.receive(Vec::new(), frame(), [None; 2], &mut Source);
        assert!(r.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
    }
    r
}

/// The items of the parts the eye does not see, in either block.
fn unseen_items(r: &Renderer) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for (id, placed) in [(NEAR_BLOCK, NEAR_PLACED), (FAR_BLOCK, FAR_PLACED)] {
        for (k, p) in placed.iter().enumerate() {
            if !p.4 {
                for level in r.part_items(id, k as u32) {
                    out.extend(level);
                }
            }
        }
    }
    out
}

/// The items of the parts that turn toward the viewer, in either block.
fn turning_items(r: &Renderer) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    for (id, placed) in [(NEAR_BLOCK, NEAR_PLACED), (FAR_BLOCK, FAR_PLACED)] {
        for (k, p) in placed.iter().enumerate() {
            if p.3 {
                for level in r.part_items(id, k as u32) {
                    out.extend(level);
                }
            }
        }
    }
    out
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
fn edges_and_blended_surfaces_sorted_on_the_device_are_the_processors_lists_and_frame() {
    let mut cpu = drawn(false);
    assert_eq!(cpu.parts_on_device(), 0);
    let unseen = unseen_items(&cpu);
    // The processor lists every part of a drawn block; the device only those in the view, which
    // draw the same.
    let [cpu_edges, cpu_forward] = cpu.blended_items().map(|l| {
        l.into_iter()
            .filter(|i| !unseen.contains(i))
            .collect::<Vec<_>>()
    });
    let cpu_buckets = cpu.culled_items().map(|l| {
        l.into_iter()
            .filter(|i| !unseen.contains(i))
            .collect::<Vec<_>>()
    });
    let cpu_frame = cpu.read_back().expect("the processor's frame");
    drop(cpu);

    let mut gpu = drawn(true);
    let turning = turning_items(&gpu);
    let placed = NEAR_PLACED.len() + FAR_PLACED.len();
    let turns = NEAR_PLACED.iter().chain(FAR_PLACED).filter(|p| p.3).count();
    assert_eq!(
        gpu.parts_on_device(),
        placed - turns,
        "all but the turning parts"
    );
    assert!(
        gpu.sorts_blended(),
        "the device blends a list of both blends in one pipeline"
    );
    let [edges, forward] = gpu.blended_items();
    // The scene reaches every case: edges and blended and added surfaces of the device's parts
    // and the processor's, ties broken by item.
    assert!(edges.len() >= 8, "{edges:?}");
    assert!(forward.len() >= 8, "{forward:?}");
    assert!(
        edges.iter().any(|i| turning.contains(i)),
        "the processor's edges among them"
    );
    assert!(
        forward.iter().any(|i| turning.contains(i)),
        "the processor's blended among them"
    );
    assert_eq!(edges, cpu_edges, "the cut-outs' edges, far to near");
    assert_eq!(forward, cpu_forward, "the blended surfaces, far to near");
    let buckets = gpu.culled_items();
    assert!(buckets[1].iter().any(|i| turning.contains(i)));
    assert_eq!(
        buckets, cpu_buckets,
        "the G-buffer's buckets in the walk's order"
    );
    let gpu_frame = gpu.read_back().expect("the device's frame");
    let differ = differing(&cpu_frame, &gpu_frame);
    assert!(
        differ.iter().all(|(_, n)| *n == 0),
        "bytes differing: {differ:?}"
    );

    // The same frame again lists the same commands in the same order.
    gpu.receive(Vec::new(), frame(), [None; 2], &mut Source);
    assert!(gpu.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
    assert_eq!(gpu.blended_items(), [edges, forward], "the same order");
    assert_eq!(gpu.culled_items(), buckets, "the same order");
    drop(gpu);
    crate::no_validation_errors();
}
