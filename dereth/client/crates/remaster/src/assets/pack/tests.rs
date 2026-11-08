//! Behaviour: none (experimental)

use glam::{Mat4, Vec3};

use super::*;
use crate::assets::{
    kind, AssetSource, AssetTables, BlendFactor, CellPieceKey, Kind, Material, MaterialKey, Mesh,
    MeshKey, SubsetInfo, SurfaceRef, Texture, TextureFormat, TextureKey, Use, Vertex,
};
use crate::scene::{PartLook, Placement, PlacementKind, Terrain};

/// Off, nothing of a pack is drawn; every switch on is every kind; each kind's models follow
/// their own switch.
#[test]
fn the_switches_are_off_until_turned_on_each_kind_by_its_own() {
    let off = PackSettings::default();
    assert!(!off.any());
    assert!(Category::ALL.iter().all(|c| !off.models(*c)));
    let all = PackSettings::all();
    assert!(Category::ALL.iter().all(|c| all.models(*c)));
    let water = PackSettings {
        water: true,
        ..PackSettings::default()
    };
    assert!(water.any());
    assert_eq!(
        Category::ALL.map(|c| water.models(c)),
        [false, false, false, true]
    );
}

/// The pack's names read back as what they name, and none is a retail id: GfxObjs, Setups,
/// pictures, render surfaces and surfaces all stay below them.
#[test]
fn the_packs_names_read_back_and_are_past_every_retail_id() {
    assert_eq!(of_pack_mesh(pack_mesh(4321, 2)), Some((4321, 2)));
    assert_eq!(
        of_pack_surface(pack_surface(4000, 3, 17)),
        Some((4000, 3, 17))
    );
    assert_eq!(of_pack_picture(pack_picture(70_000)), Some(70_000));
    for retail in [
        0x0100_1234,
        0x0200_0258,
        0x0500_287C,
        0x0600_1234,
        0x0800_0001,
    ] {
        assert_eq!(of_pack_mesh(retail), None);
        assert_eq!(of_pack_surface(retail), None);
        assert_eq!(of_pack_picture(retail), None);
    }
    assert_eq!(of_pack_mesh(pack_surface(1, 0, 0)), None);
    assert_eq!(of_pack_picture(pack_mesh(1, 0)), None);
}

fn placement(id: u32, frame: Mat4, scale: f32, parts: Vec<Part>) -> Placement {
    Placement {
        lifts: [0.0; 2],
        id,
        kind: PlacementKind::Object,
        frame,
        scale,
        parts,
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

fn flat() -> Terrain {
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
        words: [4; 81],
    }
}

const TREE: u32 = 0x0200_0258;

/// A block of a tree the pack has a model for (two parts, one degrading) and a rock it has none
/// for.
fn block() -> Block {
    let at = Mat4::from_translation(Vec3::new(10.0, 20.0, 3.0));
    Block {
        block: 0xA9B4,
        generation: 1,
        terrain: flat(),
        placements: vec![
            placement(TREE, at, 1.5, vec![part(2, Some(0)), part(3, None)]),
            placement(0x0100_0A7E, Mat4::IDENTITY, 1.0, vec![part(5, None)]),
        ],
        degrades: vec![Degrade {
            levels: vec![2, 4, 0],
            ..Degrade::default()
        }],
        rooms: Vec::new(),
        lights: Vec::new(),
        lamps: Vec::new(),
    }
}

/// The tree's model: three levels, the last drawn at any distance.
fn tree_model() -> PackModel {
    PackModel {
        category: Category::Tree,
        levels: (0..3)
            .map(|k| PackLevel {
                mesh: pack_mesh(7, k),
                distance: if k == 2 {
                    f32::INFINITY
                } else {
                    40.0 * f32::from(k + 1)
                },
            })
            .collect(),
    }
}

/// A placement the pack has a model for becomes one part at its own frame and scale, its mesh
/// the model's nearest level, its levels a degrade record after the block's own; the rest of
/// the block is as it was.
#[test]
fn a_placement_the_pack_has_a_model_for_is_drawn_as_the_model() {
    let mut b = block();
    let before = block();
    let swapped = swap_placements(&mut b, &mut |id| (id == TREE).then(tree_model));
    assert_eq!(swapped, 1);
    let tree = &b.placements[0];
    assert_eq!(tree.parts.len(), 1);
    assert_eq!(tree.parts[0].gfxobj, pack_mesh(7, 0));
    assert_eq!(tree.parts[0].frame, before.placements[0].frame);
    assert_eq!(tree.parts[0].scale, Vec3::splat(1.5));
    assert_eq!(tree.parts[0].degrade, Some(1));
    assert_eq!(
        b.degrades[0], before.degrades[0],
        "the block's own records stay"
    );
    assert_eq!(
        b.degrades[1].levels,
        [pack_mesh(7, 0), pack_mesh(7, 1), pack_mesh(7, 2)]
    );
    assert_eq!(
        b.degrades[1].centre,
        Vec3::new(10.0, 20.0, 3.0),
        "measured from the model"
    );
    assert_eq!(b.degrades[1].scale_z, 1.5);
    assert!(b.degrades[1].bands.is_empty());
    assert_eq!(
        b.placements[1], before.placements[1],
        "the rock has no model"
    );
    assert_eq!(b.terrain, before.terrain);
    // With no model at all, the deltas are as they came.
    let deltas = vec![SceneDelta::BlockIn(Box::new(block()))];
    assert_eq!(swap_deltas(deltas.clone(), &mut |_| None), deltas);
    // A model of one level needs no degrade record.
    let mut b = block();
    let one = |_| {
        Some(PackModel {
            category: Category::Rock,
            levels: vec![PackLevel {
                mesh: pack_mesh(3, 0),
                distance: f32::INFINITY,
            }],
        })
    };
    swap_placements(&mut b, &mut |id| {
        (id == 0x0100_0A7E).then(|| one(id)).flatten()
    });
    assert_eq!(b.placements[1].parts, vec![part(pack_mesh(3, 0), None)]);
    assert_eq!(b.degrades.len(), 1);
}

/// A source with a pack: the tree's model of three levels, each a triangle on a surface of its
/// own drawing the pack's picture of the level's number; and retail's GfxObjs, each a triangle on
/// surface `id * 10`, drawing picture `id`.
#[derive(Debug, Default)]
struct WithPack {
    pictures: Vec<TextureKey>,
}

fn triangle() -> Vec<Vertex> {
    [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        .into_iter()
        .map(|p| Vertex {
            position: p,
            normal: [0.0, 0.0, 1.0],
            uv: [p[0], p[1]],
        })
        .collect()
}

impl AssetSource for WithPack {
    fn pack_model(&mut self, id: u32) -> Option<PackModel> {
        (id == TREE).then(tree_model)
    }

    fn gfxobj(&mut self, id: u32) -> Option<Mesh> {
        let surface = match of_pack_mesh(id) {
            Some((model, level)) => pack_surface(model, level, 0),
            None => id * 10,
        };
        Some(Mesh::from_triangle_lists([(
            SubsetInfo {
                surface: SurfaceRef::Surface(surface),
                two_sided: true,
                tiled: true,
            },
            triangle(),
        )]))
    }

    fn cell_piece(&mut self, _key: CellPieceKey) -> Option<Mesh> {
        None
    }

    fn appearance(&mut self, _surface: u32, _look: Option<&PartLook>) -> crate::assets::Appearance {
        crate::assets::Appearance::default()
    }

    fn material(&mut self, key: &MaterialKey, used: Use) -> Material {
        let picture = match of_pack_surface(key.surface) {
            Some((_, level, _)) => pack_picture(u32::from(level)),
            None => key.surface / 10,
        };
        let mut m = Material {
            kind: Kind::Item,
            surface_type: 2,
            data_category: Some(1),
            texture: Some(TextureKey::Dat {
                picture,
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
        };
        m.kind = kind::of(&m, used);
        m
    }

    fn texture(&mut self, key: &TextureKey, _clip_map: bool) -> Option<Texture> {
        self.pictures.push(key.clone());
        Some(Texture {
            width: 1,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0; 4]],
        })
    }

    fn terrain_cells(
        &mut self,
        _block: crate::scene::BlockId,
        _t: &Terrain,
    ) -> Vec<crate::assets::TerrainCell> {
        Vec::new()
    }

    fn sky_objects(&mut self) -> Vec<u32> {
        Vec::new()
    }

    fn sky(&mut self, _id: u32) -> Option<Mesh> {
        None
    }
}

/// Every one of a pack's assets the swapped block uses resolves: the model's three levels, their
/// surfaces and their pictures are held beside the rock's retail ones, and none of the tree's
/// retail GfxObjs is; the swaps count them so, and know how far each level is drawn.
#[test]
fn every_pack_asset_a_swapped_block_uses_is_held() {
    let mut src = WithPack::default();
    let mut swaps = PackSwaps {
        settings: PackSettings::all(),
        ..PackSwaps::default()
    };
    let deltas = swaps.swap(vec![SceneDelta::BlockIn(Box::new(block()))], &mut |id| {
        src.pack_model(id)
    });
    let mut tables = AssetTables::new();
    for d in &deltas {
        tables.apply(d, &mut src);
    }
    for k in 0..3 {
        assert!(
            tables.mesh(MeshKey::GfxObj(pack_mesh(7, k))).is_some(),
            "level {k}"
        );
        assert!(
            tables
                .material(&MaterialKey::plain(pack_surface(7, k, 0)))
                .is_some(),
            "level {k}'s surface"
        );
        assert!(src.pictures.contains(&TextureKey::Dat {
            picture: pack_picture(u32::from(k)),
            palette: 0
        }));
    }
    assert!(tables.mesh(MeshKey::GfxObj(5)).is_some(), "the rock's own");
    for retail in [2, 3, 4] {
        assert!(tables.mesh(MeshKey::GfxObj(retail)).is_none(), "{retail}");
    }
    assert_eq!(swaps.distance(pack_mesh(7, 1)), Some(80.0));
    assert_eq!(swaps.distance(pack_mesh(7, 2)), Some(f32::INFINITY));
    assert_eq!(swaps.distance(5), None);
    assert_eq!(
        swaps.counts(&tables),
        PackCounts {
            placements: 1,
            meshes: 3,
            meshes_held: 3,
            surfaces: 3,
            surfaces_held: 3,
            pictures: 3,
            pictures_held: 3,
        }
    );
}

/// A green leaf in the middle of a picture whose cut-away texels are a magenta key, as a painted
/// picture often leaves them: once filled, no texel of the picture nor of any level averaged
/// from it carries the key's color, its alpha is the picture's own, and a picture the cut keeps
/// nothing of is left as it was.
#[test]
fn a_cut_out_pictures_cut_away_texels_carry_no_other_color_into_its_edge_at_any_level() {
    let (w, h) = (32u32, 32u32);
    let mut rgba = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let leaf = (8..20).contains(&x) && (10..26).contains(&y);
            rgba.extend(if leaf {
                [40, 200, 60, 255]
            } else {
                [255, 0, 255, 0]
            });
        }
    }
    let alpha = |l: &[u8]| {
        l.as_chunks::<4>()
            .0
            .iter()
            .map(|t| t[3])
            .collect::<Vec<_>>()
    };
    let mut filled = rgba.clone();
    fill_cut_away(&mut filled, w, h, 0.5);
    assert_eq!(alpha(&filled), alpha(&rgba));
    let magenta = |t: &[u8; 4]| t[0] > 60 || t[2] > 80;
    // Each level the mean of four texels of the one before, as a picture's levels are made.
    let (mut level, mut lw, mut lh) = (filled, w as usize, h as usize);
    loop {
        let tinted = level
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|t| magenta(t))
            .count();
        assert_eq!(
            tinted, 0,
            "{lw} by {lh}: {tinted} texels carry the key's color"
        );
        if lw == 1 {
            break;
        }
        let (nw, nh) = (lw / 2, lh / 2);
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let s: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                        .iter()
                        .map(|(dx, dy)| u32::from(level[((y * 2 + dy) * lw + x * 2 + dx) * 4 + c]))
                        .sum();
                    next[(y * nw + x) * 4 + c] = u8::try_from(s / 4).unwrap_or(255);
                }
            }
        }
        (level, lw, lh) = (next, nw, nh);
    }
    let mut none_kept = [255, 0, 255, 0].repeat(16);
    let was = none_kept.clone();
    fill_cut_away(&mut none_kept, 4, 4, 0.5);
    assert_eq!(none_kept, was);
}
