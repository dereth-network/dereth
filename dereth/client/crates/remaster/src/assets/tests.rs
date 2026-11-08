//! Behaviour: none (experimental)

use std::collections::BTreeMap;

use glam::{Mat4, Vec3};

use super::*;
use crate::scene::{
    Block, Degrade, Object, ObjectKey, ObjectPart, Part, Placement, PlacementKind, Room, SceneDelta,
};

/// A stand-in for the dats: each GfxObj `id` is one triangle on surface `id * 10` and, when `id`
/// is even, a second on surface `id * 10 + 1`; surface `s` draws picture `s / 10` (so a GfxObj's
/// two surfaces share one picture); a look's texture swap names the picture directly. It counts
/// what it is asked.
#[derive(Debug, Default)]
struct Dats {
    asked: BTreeMap<&'static str, u32>,
}

impl Dats {
    fn ask(&mut self, what: &'static str) {
        *self.asked.entry(what).or_insert(0) += 1;
    }
}

fn triangle(z: f32) -> Vec<Vertex> {
    [[0.0, 0.0, z], [1.0, 0.0, z], [0.0, 1.0, z]]
        .into_iter()
        .map(|p| Vertex {
            position: p,
            normal: [0.0, 0.0, 1.0],
            uv: [p[0], p[1]],
        })
        .collect()
}

fn info(surface: SurfaceRef) -> SubsetInfo {
    SubsetInfo {
        surface,
        two_sided: false,
        tiled: false,
    }
}

impl AssetSource for Dats {
    fn gfxobj(&mut self, id: u32) -> Option<Mesh> {
        self.ask("gfxobj");
        if id == 99 {
            return None;
        }
        let mut lists = vec![(info(SurfaceRef::Surface(id * 10)), triangle(0.0))];
        if id.is_multiple_of(2) {
            lists.push((info(SurfaceRef::Surface(id * 10 + 1)), triangle(1.0)));
        }
        Some(Mesh::from_triangle_lists(lists))
    }

    fn cell_piece(&mut self, key: CellPieceKey) -> Option<Mesh> {
        self.ask("cell piece");
        Some(Mesh::from_triangle_lists((0..=key.structure).map(|s| {
            (info(SurfaceRef::Slot(s)), triangle(f32::from(s)))
        })))
    }

    fn appearance(&mut self, surface: u32, look: Option<&PartLook>) -> Appearance {
        let Some(look) = look else {
            return Appearance::default();
        };
        Appearance {
            texture: look
                .textures
                .iter()
                .find(|(old, _)| *old == surface / 10)
                .map(|(_, new)| *new),
            palette: look.palette.map(|base| PaletteRecipe {
                base,
                ranges: look.ranges.clone(),
            }),
        }
    }

    fn material(&mut self, key: &MaterialKey, used: Use) -> Material {
        self.ask("material");
        let picture = key.appearance.texture.unwrap_or(key.surface / 10);
        let texture = match &key.appearance.palette {
            Some(p) => TextureKey::Shifted {
                picture,
                palette: p.clone(),
                clip_map: false,
            },
            None => TextureKey::Dat {
                picture,
                palette: 0,
            },
        };
        let mut m = Material {
            kind: Kind::Item,
            surface_type: 2,
            data_category: Some(4),
            texture: Some(texture),
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
        };
        m.kind = kind::of(&m, used);
        m
    }

    fn texture(&mut self, key: &TextureKey, _clip_map: bool) -> Option<Texture> {
        self.ask("texture");
        let size = match key {
            TextureKey::Dat { picture, .. } | TextureKey::Shifted { picture, .. } => *picture,
            TextureKey::Solid(_) | TextureKey::Terrain(_) | TextureKey::TerrainSource { .. } => 1,
        };
        Some(Texture {
            width: size.max(1),
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0; 4 * usize::try_from(size.max(1)).unwrap()]],
        })
    }

    fn sky_objects(&mut self) -> Vec<u32> {
        vec![0x0100_0010, 0x0200_0020]
    }

    /// Key `k`'s splat: tile `k` under one overlay, whose alpha map every key shares, laying
    /// tile `k + 1`.
    fn splat(&mut self, key: u32) -> Option<terrain::Splat> {
        self.ask("splat");
        let source = |picture| TextureKey::TerrainSource {
            picture,
            texels: 512,
        };
        Some(terrain::Splat {
            base: Some(source(key)),
            base_tiling: 1,
            base_ground: None,
            base_maps: None,
            base_broad: None,
            slopes: Vec::new(),
            bank: None,
            overlays: vec![terrain::SplatOverlay {
                alpha: source(1000),
                rotation: 1,
                tile: Some(source(key + 1)),
                tiling: 2,
                ground: None,
                maps: None,
                broad: None,
            }],
        })
    }

    fn sky(&mut self, id: u32) -> Option<Mesh> {
        self.ask("sky");
        Some(Mesh::from_triangle_lists([(
            info(SurfaceRef::Surface(id & 0xFF)),
            triangle(9.0),
        )]))
    }

    fn terrain_cells(
        &mut self,
        _block: crate::scene::BlockId,
        terrain: &Terrain,
    ) -> Vec<TerrainCell> {
        self.ask("terrain");
        let n = usize::from(terrain.side) * usize::from(terrain.side);
        (0..n)
            .map(|i| TerrainCell {
                texture: TextureKey::Terrain(u32::from(terrain.words[i % 81])),
                rotation: 0,
                kind: Kind::Ground,
            })
            .collect()
    }
}

/// A sink that records what it is handed, in order.
#[derive(Debug, Default)]
struct Device {
    log: Vec<String>,
}

impl AssetSink for Device {
    fn texture(&mut self, key: &TextureKey, _texture: &Texture) {
        self.log.push(format!("texture {key:?}"));
    }
    fn material(&mut self, key: &MaterialKey, _material: &Material) {
        self.log.push(format!("material {}", key.surface));
    }
    fn mesh(&mut self, key: MeshKey, _mesh: &Mesh) {
        self.log.push(format!("mesh {key:?}"));
    }
    fn terrain(&mut self, block: crate::scene::BlockId, _surface: &TerrainSurface) {
        self.log.push(format!("terrain {block}"));
    }
    fn splat(&mut self, key: u32, _splat: &terrain::Splat) {
        self.log.push(format!("splat {key}"));
    }
    fn release(&mut self, key: &AssetKey) {
        self.log.push(format!("release {key:?}"));
    }
}

fn flat(words: u16) -> Terrain {
    let mut vertices = Vec::new();
    for i in 0..9u8 {
        for j in 0..9u8 {
            vertices.push(Vec3::new(f32::from(i) * 24.0, f32::from(j) * 24.0, 0.0));
        }
    }
    Terrain {
        side: 8,
        vertices,
        cuts: vec![false; 64],
        words: [words; 81],
    }
}

fn part(gfxobj: u32, degrade: Option<u32>) -> Part {
    Part {
        gfxobj,
        frame: Mat4::IDENTITY,
        scale: Vec3::ONE,
        degrade,
    }
}

fn placement(parts: Vec<Part>) -> Placement {
    Placement {
        lifts: [0.0; 2],
        id: 0x0200_0001,
        kind: PlacementKind::Object,
        frame: Mat4::IDENTITY,
        scale: 1.0,
        parts,
    }
}

/// A block of two placements sharing GfxObj 2, one of them degrading to 4 and then to nothing,
/// and a cell built from piece (7, 1) naming surfaces 30 and 31.
fn block(id: crate::scene::BlockId) -> Block {
    Block {
        block: id,
        generation: 1,
        terrain: flat(4),
        placements: vec![
            placement(vec![part(2, None), part(3, None)]),
            placement(vec![part(2, Some(0))]),
        ],
        degrades: vec![Degrade {
            levels: vec![2, 4, 0],
            ..Degrade::default()
        }],
        rooms: vec![Room {
            id: (u32::from(id) << 16) | 0x0100,
            environment: 7,
            structure: 1,
            surfaces: vec![30, 31],
            frame: Mat4::IDENTITY,
            seen_outside: false,
            portals: Vec::new(),
            placements: vec![placement(vec![part(3, None)])],
        }],
        lights: Vec::new(),
        lamps: Vec::new(),
    }
}

/// **A block holds each mesh, material and texture once however often it uses it**, every
/// degrade level is a mesh of its own, a cell's piece names its surfaces by the cell's list, and
/// what the dats are asked is exactly what is held.
#[test]
fn a_block_holds_each_entry_once_and_asks_the_dats_once_for_each() {
    let mut dats = Dats::default();
    let mut t = AssetTables::new();
    t.block_in(&block(0xA9B4), &mut dats);
    let c = t.counts();
    // GfxObjs 2, 3 and 4 (the degrade record's 2 again, and its empty level, add none).
    assert_eq!((c.meshes, c.cell_pieces, c.meshes_missing), (3, 1, 0));
    // Surfaces 20, 21, 30 (GfxObj 3's, and the cell's slot 0), 40, 41, and the cell's 31.
    assert_eq!(c.materials, 6);
    assert_eq!(c.surface_groups, 6);
    // Pictures 2 (surfaces 20, 21), 3 (30, 31), 4 (40, 41), and the one ground picture.
    assert_eq!((c.textures, c.terrain_textures), (4, 1));
    assert_eq!(c.terrain_blocks, 1);
    assert_eq!(c.triangles, 2 + 1 + 2 + 2);
    assert_eq!(dats.asked["gfxobj"], 3);
    assert_eq!(dats.asked["cell piece"], 1);
    assert_eq!(dats.asked["material"], 6);
    assert_eq!(dats.asked["texture"], 4);
    assert_eq!(
        t.cell_materials(0xA9B4_0100),
        Some(&[MaterialKey::plain(30), MaterialKey::plain(31)][..])
    );
    // Shared geometry: each mesh's identical corners are one vertex.
    let m = t.mesh(MeshKey::GfxObj(2)).unwrap();
    assert_eq!(
        (m.vertices.len(), m.indices.len(), m.subsets.len()),
        (6, 6, 2)
    );
}

/// **Entries two blocks share stay until the last of them goes, and every entry goes with its
/// last user**; an upload hands over what was made, textures first and the ground last, and
/// then releases only what it had handed over.
#[test]
fn entries_go_with_their_last_user_and_the_sink_hears_of_each_once() {
    let mut dats = Dats::default();
    let mut t = AssetTables::new();
    let mut dev = Device::default();
    t.block_in(&block(0xA9B4), &mut dats);
    t.block_in(&block(0xA9B5), &mut dats);
    assert_eq!(t.counts().meshes, 3, "the second block shares the first's");
    assert_eq!(dats.asked["gfxobj"], 3, "and the dats are not asked again");
    t.upload(&mut dev);
    let first_mesh = dev.log.iter().position(|l| l.starts_with("mesh")).unwrap();
    let last_texture = dev
        .log
        .iter()
        .rposition(|l| l.starts_with("texture"))
        .unwrap();
    let first_terrain = dev
        .log
        .iter()
        .position(|l| l.starts_with("terrain"))
        .unwrap();
    assert!(
        last_texture < first_mesh && first_mesh < first_terrain,
        "{:?}",
        dev.log
    );
    assert_eq!(t.uploads().meshes, 4);
    assert_eq!(t.uploads().terrain, 2);
    dev.log.clear();

    t.apply(&SceneDelta::BlockOut(0xA9B4), &mut dats);
    t.upload(&mut dev);
    assert_eq!(
        dev.log,
        ["release Terrain(43444)"],
        "only its own ground goes"
    );
    dev.log.clear();
    t.apply(&SceneDelta::BlockOut(0xA9B5), &mut dats);
    t.upload(&mut dev);
    assert_eq!(
        dev.log.iter().filter(|l| l.starts_with("release")).count(),
        1 + 4 + 6 + 4,
        "{:?}",
        dev.log
    );
    assert_eq!(t.counts(), AssetCounts::default());

    // Made and gone before an upload: the sink never hears of it.
    dev.log.clear();
    t.block_in(&block(1), &mut dats);
    t.apply(&SceneDelta::BlockOut(1), &mut dats);
    t.upload(&mut dev);
    assert!(dev.log.is_empty(), "{:?}", dev.log);
}

/// **A look makes its own materials only for the surfaces it changes**: a texture swap and a
/// shift palette give the swapped surface a material and a picture of its own, the object's
/// untouched surfaces share the block's, and a GfxObj the dat lacks is held as missing, not
/// asked for again.
#[test]
fn a_look_makes_materials_only_for_the_surfaces_it_changes() {
    let mut dats = Dats::default();
    let mut t = AssetTables::new();
    t.block_in(&block(0xA9B4), &mut dats);
    let before = t.counts();
    let object = |key, look| Object {
        key,
        setup: 0x0200_0002,
        parts: vec![
            ObjectPart {
                levels: vec![2],
                look,
            },
            ObjectPart {
                levels: vec![99],
                look: None,
            },
        ],
    };
    // Swaps picture 2 for 50 on GfxObj 2's surfaces: both of them change.
    let swap = PartLook {
        textures: vec![(2, 50)],
        palette: None,
        ranges: Vec::new(),
    };
    t.apply(
        &SceneDelta::ObjectBuilt(Box::new(object(ObjectKey::Server(1), Some(swap.clone())))),
        &mut dats,
    );
    let c = t.counts();
    assert_eq!(c.materials, before.materials + 2);
    assert_eq!(c.textures, before.textures + 1, "the swapped picture, once");
    assert_eq!(c.meshes_missing, 1);
    // The same look on a second object makes nothing new.
    t.apply(
        &SceneDelta::ObjectBuilt(Box::new(object(ObjectKey::Server(2), Some(swap)))),
        &mut dats,
    );
    let c = t.counts();
    assert_eq!(c.materials, before.materials + 2);
    assert_eq!(dats.asked["gfxobj"], 4, "GfxObj 99 is asked once");
    // A shift palette makes the shifted pictures.
    let dyed = PartLook {
        textures: Vec::new(),
        palette: Some(0x0400_0001),
        ranges: vec![(0x0F00_0001, 0, 8)],
    };
    t.apply(
        &SceneDelta::ObjectBuilt(Box::new(object(ObjectKey::Server(3), Some(dyed)))),
        &mut dats,
    );
    let dyed_counts = t.counts();
    assert_eq!(dyed_counts.materials, c.materials + 2);
    assert_eq!(dyed_counts.textures, c.textures + 1);
    let mats = t.object_materials(ObjectKey::Server(3), 0, 0).unwrap();
    assert!(mats.iter().all(|m| m.appearance.palette.is_some()));
    assert_eq!(
        t.object_materials(ObjectKey::Server(3), 1, 0),
        Some(&[][..])
    );
    // Removing the dyed object takes its shifted entries with it and nothing else.
    t.apply(&SceneDelta::ObjectRemoved(ObjectKey::Server(3)), &mut dats);
    assert_eq!(t.counts(), c);
}

/// **A block built again replaces what it held**: entries only the old build used go, and the
/// ground is the new build's.
#[test]
fn a_block_built_again_replaces_what_it_held() {
    let mut dats = Dats::default();
    let mut t = AssetTables::new();
    t.block_in(&block(0xA9B4), &mut dats);
    let mut again = block(0xA9B4);
    again.generation = 2;
    again.placements.truncate(1);
    again.rooms.clear();
    again.terrain = flat(8);
    t.apply(&SceneDelta::BlockChanged(Box::new(again)), &mut dats);
    let c = t.counts();
    assert_eq!((c.meshes, c.cell_pieces), (2, 0));
    assert_eq!(c.materials, 3, "20, 21 and 30");
    assert_eq!(c.terrain_blocks, 1);
    assert_eq!(
        t.terrain(0xA9B4).unwrap().cells[0].texture,
        TextureKey::Terrain(8)
    );
    assert_eq!(t.cell_materials(0xA9B4_0100), None);
}

/// **The sky is held once, whole**: a mesh for every sky object the source names, with its
/// materials, and held again it replaces what it held.
#[test]
fn the_sky_is_held_whole_and_replaced_when_held_again() {
    let mut dats = Dats::default();
    let mut t = AssetTables::new();
    t.sky_in(&mut dats);
    let c = t.counts();
    assert_eq!((c.sky_objects, c.materials, c.blocks), (2, 2, 0));
    assert!(t.mesh(MeshKey::Sky(0x0200_0020)).is_some());
    t.sky_in(&mut dats);
    assert_eq!(t.counts(), c);
    assert_eq!(dats.asked["sky"], 4, "asked again when held again");
}

/// **The ground held as its splats' layers takes no composite picture, and shares its layers**:
/// two blocks of ground keyed 4 and 5 hold two splats and four pictures (tiles 4, 5 and 6 and the
/// shared alpha map) where composites took one picture a key; switching back and forth lets go of
/// one form and takes the other, the splats reach the sink after their textures, and both forms
/// go with the last block.
#[test]
fn the_ground_as_splats_holds_shared_layers_and_switches_whole() {
    let mut dats = Dats::default();
    let mut t = AssetTables::new();
    let mut dev = Device::default();
    let mut b = block(0xA9B4);
    b.placements.clear();
    b.rooms.clear();
    let mut c = block(0xA9B5);
    c.placements.clear();
    c.rooms.clear();
    c.terrain = flat(5);
    t.block_in(&b, &mut dats);
    t.block_in(&c, &mut dats);
    assert_eq!(
        (t.counts().terrain_textures, t.counts().splats),
        (2, 0),
        "composites, one a key"
    );
    t.upload(&mut dev);
    dev.log.clear();

    t.set_splat(true, &mut dats);
    let n = t.counts();
    assert_eq!(
        (n.terrain_textures, n.splats, n.terrain_sources),
        (0, 2, 4),
        "{n:?}"
    );
    t.upload(&mut dev);
    let last_texture = dev
        .log
        .iter()
        .rposition(|l| l.starts_with("texture"))
        .unwrap();
    let first_splat = dev.log.iter().position(|l| l.starts_with("splat")).unwrap();
    assert!(last_texture < first_splat, "{:?}", dev.log);
    assert_eq!(
        dev.log.iter().filter(|l| l.starts_with("release")).count(),
        2,
        "the two composites go: {:?}",
        dev.log
    );
    assert!(t.splat_of(4).is_some() && t.splat_of(5).is_some());

    t.set_splat(false, &mut dats);
    let n = t.counts();
    assert_eq!((n.terrain_textures, n.splats, n.terrain_sources), (2, 0, 0));
    t.set_splat(true, &mut dats);
    t.apply(&SceneDelta::BlockOut(0xA9B4), &mut dats);
    t.apply(&SceneDelta::BlockOut(0xA9B5), &mut dats);
    assert_eq!(t.counts(), AssetCounts::default());
}

/// **A mesh's tangents run the way its texture's first coordinate runs, square to the normal**,
/// with `w` 1 where the second coordinate runs as the normal crossed with the tangent and -1
/// where the picture is mirrored; a mesh that brings none has them worked out.
#[test]
fn tangents_follow_the_first_texture_coordinate_and_mark_a_mirror() {
    let quad = |flip: f32| {
        let v = |x: f32, y: f32| Vertex {
            position: [x, y, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [x, y * flip],
        };
        Mesh::from_triangle_lists([(
            info(SurfaceRef::Surface(1)),
            vec![
                v(0.0, 0.0),
                v(1.0, 0.0),
                v(1.0, 1.0),
                v(0.0, 0.0),
                v(1.0, 1.0),
                v(0.0, 1.0),
            ],
        )])
    };
    for t in tangents(&quad(1.0)) {
        assert_eq!(t, [1.0, 0.0, 0.0, 1.0]);
    }
    for t in tangents(&quad(-1.0)) {
        assert_eq!(
            t,
            [1.0, 0.0, 0.0, -1.0],
            "the second coordinate runs the other way"
        );
    }
    // A leaning normal: the tangent is made square to it.
    let mut m = quad(1.0);
    for v in &mut m.vertices {
        v.normal = [0.0, 0.6, 0.8];
    }
    for t in tangents(&m) {
        let n = glam::Vec3::new(0.0, 0.6, 0.8);
        assert!(glam::Vec3::new(t[0], t[1], t[2]).dot(n).abs() < 1e-6);
    }
}

/// Two corners alike but for their color stay two vertices, and a mesh with no colors brings none.
#[test]
fn vertices_are_shared_only_when_their_colors_match() {
    let v = |x: f32| Vertex {
        position: [x, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        uv: [0.0, 0.0],
    };
    let info = SubsetInfo {
        surface: SurfaceRef::Missing,
        two_sided: false,
        tiled: false,
    };
    let (green, gray) = ([0.1, 0.9, 0.1, 1.0], [0.5, 0.5, 0.5, 1.0]);
    let list = vec![
        (v(0.0), green),
        (v(1.0), green),
        (v(0.0), green),
        (v(0.0), gray),
    ];
    let mesh = Mesh::from_colored_triangle_lists([(info, list)]);
    assert_eq!(mesh.indices, vec![0, 1, 0, 2]);
    assert_eq!(mesh.colors, vec![green, green, gray]);
    let plain = Mesh::from_triangle_lists([(info, vec![v(0.0), v(1.0), v(0.0)])]);
    assert_eq!(plain.indices, vec![0, 1, 0]);
    assert!(plain.colors.is_empty());
}
