//! An art pack's trees sway in the wind: a tree whose vertices carry how far the wind bends them
//! stands elsewhere a moment later, and stands still with the sway off or with no bend in it.
//!
//! Behaviour: none (experimental)

use dereth_gfx::Format;
use dereth_remaster::assets::pack::{pack_mesh, Category, PackLevel, PackModel, PackSettings};
use dereth_remaster::assets::{
    tangents, Appearance, AssetSource, BlendFactor, CellPieceKey, Kind, Material, MaterialKey,
    Mesh, SubsetInfo, SurfaceRef, TerrainCell, Texture, TextureKey, Use, Vertex,
};
use dereth_remaster::render::landscape::LandscapeSettings;
use dereth_remaster::render::Renderer;
use dereth_remaster::scene::{
    Block, BlockId, BlockLevels, Camera, FrameDelta, Part, PartLook, Placement, PlacementKind,
    RetailLight, SceneDelta, Terrain,
};
use glam::{Mat4, Vec3};

const BLOCK: BlockId = 0xA9B4;
const SIZE: (u32, u32) = (320, 240);
const TREE: u32 = 0x0100_0001;
const EYE: Vec3 = Vec3::new(96.0, 20.0, 5.0);
/// The tree's height, meters.
const TOP: f32 = 10.0;

/// A tree: a card two meters wide and [`TOP`] tall, facing south, whose vertices carry how far
/// the wind bends them as the art pack's growing models do, rising with the square of the height
/// to a fiftieth of its height at its top; with `bent` false, none.
struct Source {
    bent: bool,
}

impl AssetSource for Source {
    fn gfxobj(&mut self, _id: u32) -> Option<Mesh> {
        let v = |x: f32, z: f32| Vertex {
            position: [x, 0.0, z],
            normal: [0.0, -1.0, 0.0],
            uv: [x, z / TOP],
        };
        let mut mesh = Mesh::from_triangle_lists([(
            SubsetInfo {
                surface: SurfaceRef::Surface(7),
                two_sided: true,
                tiled: false,
            },
            vec![
                v(-1.0, 0.0),
                v(1.0, 0.0),
                v(1.0, TOP),
                v(-1.0, 0.0),
                v(1.0, TOP),
                v(-1.0, TOP),
            ],
        )]);
        let mut t = tangents(&mesh);
        if self.bent {
            for (t, v) in t.iter_mut().zip(&mesh.vertices) {
                let h = (v.position[2] / TOP).clamp(0.0, 1.0);
                let bend = h * h * TOP * 0.02;
                t[3] *= 1.0 + 2.0 * (bend * 1000.0).round();
            }
        }
        mesh.tangents = t;
        Some(mesh)
    }

    fn cell_piece(&mut self, _key: CellPieceKey) -> Option<Mesh> {
        None
    }

    fn appearance(&mut self, _surface: u32, _look: Option<&PartLook>) -> Appearance {
        Appearance::default()
    }

    fn material(&mut self, _key: &MaterialKey, _used: Use) -> Material {
        Material {
            kind: Kind::Item,
            surface_type: 2,
            data_category: Some(1),
            texture: None,
            clip_map: false,
            alpha_test: None,
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

    fn texture(&mut self, _key: &TextureKey, _clip_map: bool) -> Option<Texture> {
        None
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

    fn pack_model(&mut self, id: u32) -> Option<PackModel> {
        (id == TREE).then(|| PackModel {
            category: Category::Tree,
            levels: vec![PackLevel {
                mesh: pack_mesh(0, 0),
                distance: f32::INFINITY,
            }],
            cover: false,
        })
    }
}

fn block() -> Block {
    let mut vertices = Vec::new();
    for i in 0..9u8 {
        for j in 0..9u8 {
            vertices.push(Vec3::new(f32::from(i) * 24.0, f32::from(j) * 24.0, 0.0));
        }
    }
    let frame = Mat4::from_translation(Vec3::new(EYE.x, EYE.y + 12.0, 0.0));
    Block {
        block: BLOCK,
        generation: 1,
        terrain: Terrain {
            side: 8,
            vertices,
            cuts: vec![false; 64],
            words: [4; 81],
        },
        placements: vec![Placement {
            id: TREE,
            kind: PlacementKind::Object,
            frame,
            scale: 1.0,
            lifts: [0.0; 2],
            parts: vec![Part {
                gfxobj: 1,
                frame,
                scale: Vec3::ONE,
                degrade: None,
            }],
        }],
        degrades: Vec::new(),
        rooms: Vec::new(),
        lights: Vec::new(),
        lamps: Vec::new(),
    }
}

/// The frame at `clock` seconds.
fn frame(clock: f64) -> FrameDelta {
    let aspect = SIZE.0 as f32 / SIZE.1 as f32;
    FrameDelta {
        origin: [0xA9, 0xB4],
        landscape: true,
        drawn_blocks: vec![BLOCK],
        levels: vec![BlockLevels {
            block: BLOCK,
            levels: Vec::new(),
            turned: Vec::new(),
        }],
        clock,
        camera: Camera {
            view: Mat4::look_to_lh(EYE, Vec3::Y, Vec3::Z),
            projection: Mat4::perspective_lh(1.2, aspect, 0.3, 1000.0),
            previous_view_projection: None,
            eye: EYE,
            viewport: [0, 0, SIZE.0, SIZE.1],
            fov_y: 1.2,
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

/// The depth of the tree drawn at two moments 1.3 s apart, its sway as `sway` says and its
/// vertices bent as `bent` says: how many pixels differ, and how many the tree covers.
fn moved(sway: bool, bent: bool) -> (usize, usize) {
    let mut r = Renderer::new(crate::hardware(), Format::D32Float).expect("a renderer");
    r.set_packs(PackSettings::all());
    r.set_landscape(LandscapeSettings {
        sway,
        ..LandscapeSettings::default()
    });
    let mut src = Source { bent };
    let mut depth = |r: &mut Renderer, clock: f64, deltas: Vec<SceneDelta>| {
        r.receive(deltas, frame(clock), [None; 2], &mut src);
        assert!(r.draw_offscreen(SIZE.0, SIZE.1).expect("a frame"));
        r.read_back().expect("a frame").depth
    };
    let a = depth(&mut r, 40.0, vec![SceneDelta::BlockIn(Box::new(block()))]);
    let b = depth(&mut r, 41.3, Vec::new());
    let far = 1.0f32.to_le_bytes();
    let covered = a.chunks(4).filter(|d| *d != far).count();
    let changed = a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x != y).count();
    (changed, covered)
}

#[test]
fn a_tree_bent_by_the_wind_sways_and_stands_still_with_the_sway_off_or_no_bend() {
    let (swaying, covered) = moved(true, true);
    assert!(covered > 1000, "the tree covers {covered} pixels");
    assert!(
        swaying > covered / 50,
        "{swaying} of the tree's {covered} pixels moved"
    );
    assert_eq!(moved(false, true).0, 0, "with the sway off it stands still");
    assert_eq!(moved(true, false).0, 0, "with no bend it stands still");
    crate::no_validation_errors();
}
