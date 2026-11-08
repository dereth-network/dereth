//! Behaviour: none (experimental)
//!
//! The remaster's assets against Retail Look's own, at the Holtburg and forest stations: the same
//! scene is loaded on a device, and what Retail Look decoded and uploaded for it is read back and
//! built again through [`DatAssets`], which has no device.

use std::collections::{BTreeMap, BTreeSet};

use dereth_remaster::assets::{kind, AssetSource, AssetTables, Kind, MaterialKey, TextureKey};
use dereth_remaster::scene::{Object, ObjectKey, ObjectPart, Terrain};

use super::super::*;
use super::*;

/// Holtburg.
const HOLTBURG: u16 = 0xA9B4;

/// **An `expect`, never a skip.**
fn store() -> std::sync::Arc<RetailDatStore> {
    std::sync::Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "no retail dats under {} -- set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    }))
}

fn device() -> Gpu {
    let cfg = dereth_render::device::DeviceConfig {
        width: 64,
        height: 64,
        ..dereth_render::device::DeviceConfig::default()
    };
    Gpu::new(None, &cfg).expect("a graphics device")
}

/// A station's scene, as the stations load it: the window three blocks round `block`, built at
/// once, with no body.
fn station(store: &RetailDatStore, gpu: &mut Gpu, block: u16) -> WorldScene {
    let mut cfg = SceneConfig {
        landblock: block,
        character: false,
        land_radius: 3,
        ..SceneConfig::default()
    };
    cfg.render.automatic_degrades = false;
    WorldScene::load(store, gpu, cfg).expect("the station's scene loads")
}

/// The block within four of Holtburg with the most scenery-bearing vertices on dry land: the
/// forest station's.
fn forest(store: &RetailDatStore) -> u16 {
    let (hx, hy) = dereth_world_data::landblock::block_xy(HOLTBURG);
    let mut best = (0usize, HOLTBURG);
    for dx in -4..=4 {
        for dy in -4..=4 {
            let (Ok(x), Ok(y)) = (u16::try_from(hx + dx), u16::try_from(hy + dy)) else {
                continue;
            };
            let id = (x << 8) | y;
            if id == HOLTBURG {
                continue;
            }
            let did = dereth_world_data::landblock::landblock_did(id);
            let Some(lb) = store
                .read_typed(DbType::LandBlock, did)
                .ok()
                .and_then(|b| CellLandblock::decode_payload(did, &b).ok())
            else {
                continue;
            };
            let n = lb
                .terrain
                .iter()
                .filter(|w| (*w >> 11) & 0x1F != 0 && (*w >> 2) & 0x1F < 16)
                .count();
            if n > best.0 {
                best = (n, id);
            }
        }
    }
    best.1
}

/// The stations, by name and block.
fn stations(store: &RetailDatStore) -> [(&'static str, u16); 2] {
    [("holtburg", HOLTBURG), ("forest", forest(store))]
}

/// Every resident block's drawn grid as the remaster is sent it, with its id, Retail Look's own
/// normals for it and the merge keys Retail Look drew its cells with.
fn grids(scene: &WorldScene) -> Vec<(u16, Terrain, Vec<Vec3>, Vec<u32>)> {
    let window = &scene.world.streamer.window;
    let mut out = Vec::new();
    for xi in 0..window.mid_width() {
        for yi in 0..window.mid_width() {
            let Some(slot) = window.slot(xi, yi) else {
                continue;
            };
            let Some(mesh) = slot
                .mesh
                .as_ref()
                .and_then(dereth_client_runtime::world_stream::SlotMesh::get::<LandblockMesh>)
            else {
                continue;
            };
            let Some(block) = scene.draw.blocks.get(&(slot.block_x, slot.block_y)) else {
                continue;
            };
            let (Ok(x), Ok(y)) = (u16::try_from(slot.block_x), u16::try_from(slot.block_y)) else {
                continue;
            };
            let terrain = Terrain {
                side: mesh.side_cell_count,
                vertices: mesh
                    .vertices
                    .iter()
                    .map(|v| glam::Vec3::new(v.x, v.y, v.z))
                    .collect(),
                cuts: mesh.sw_to_ne_cut.clone(),
                words: block.terrain,
            };
            let normals = dereth_terrain::land::lighting::vertex_normals(mesh);
            let keys = block.merge_keys.iter().map(|k| k.0).collect();
            out.push(((x << 8) | y, terrain, normals, keys));
        }
    }
    out
}

/// **The ground's normals are retail's, to the bit, and each drawn cell's picture is the one
/// Retail Look drew it with**, at every resident block of both stations, so the remaster holds as
/// many ground pictures as Retail Look's merge cache.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn the_grounds_normals_and_cell_pictures_are_retail_looks_at_the_stations() {
    let store = store();
    let mut gpu = device();
    for (name, block) in stations(&store) {
        let mut scene = station(&store, &mut gpu, block);
        let mut src = DatAssets::for_scene(&store, &scene.draw);
        let grids = grids(&scene);
        assert!(grids.len() >= 25, "{name}: {} blocks resident", grids.len());
        let mut pictures = BTreeSet::new();
        for (id, terrain, normals, keys) in &grids {
            let ours = dereth_remaster::assets::terrain::normals(terrain);
            assert_eq!(ours.len(), normals.len(), "{name} {id:04X}");
            for (k, (a, b)) in ours.iter().zip(normals).enumerate() {
                assert_eq!(*a, [b.x, b.y, b.z], "{name} {id:04X} vertex {k}");
            }
            let cells = src.terrain_cells(*id, terrain);
            let ours: Vec<u32> = cells
                .iter()
                .map(|c| match c.texture {
                    TextureKey::Terrain(k) => k,
                    ref other => panic!("a ground cell names {other:?}"),
                })
                .collect();
            assert_eq!(&ours, keys, "{name} {id:04X}: the cells' merge keys");
            pictures.extend(ours);
        }
        assert_eq!(
            pictures.len(),
            scene.draw.land.merge.len(),
            "{name}: one ground picture a merge key, as Retail Look composites them"
        );
        // A ground picture is every level down to one texel, its first Retail Look's own.
        let first = *pictures.first().expect("the ground has pictures");
        let t = src
            .texture(&TextureKey::Terrain(first), false)
            .expect("the ground's picture composites");
        assert_eq!(t.levels.len(), 1 + t.width.max(t.height).ilog2() as usize);
        scene.release_textures(&mut gpu);
    }
}

/// What Retail Look resolved: each surface group as the remaster keys its material.
fn resolved(draw: &SceneDraw) -> Vec<(MaterialKey, bool, bool)> {
    groups_of(&draw.land.bake)
}

/// What a surface cache resolved: each surface group as the remaster keys its material.
fn groups_of(cache: &BakeCache) -> Vec<(MaterialKey, bool, bool)> {
    cache
        .surfaces
        .keys()
        .map(|g| {
            (
                MaterialKey {
                    surface: g.surface.map_or(0, |s| s.0),
                    appearance: dereth_remaster::assets::Appearance {
                        texture: g.appearance.texture.map(|t| t.0),
                        palette: g.appearance.palette.as_ref().map(|p| {
                            dereth_remaster::assets::PaletteRecipe {
                                base: p.base.0,
                                ranges: p.ranges.iter().map(|&(s, o, n)| (s.0, o, n)).collect(),
                            }
                        }),
                    },
                },
                g.two_sided,
                g.tiled,
            )
        })
        .collect()
}

/// **Every surface Retail Look resolved at the stations is a material whose picture is Retail
/// Look's**: as many distinct pictures as Retail Look uploaded through its surface cache, each
/// of them decoding, and every data category the stations' pictures carry has a kind of its own
/// in the table, not a fallback.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn every_surface_retail_look_resolved_is_a_material_with_retail_looks_picture() {
    let store = store();
    let mut gpu = device();
    let mut census: BTreeMap<(Option<u32>, Kind), usize> = BTreeMap::new();
    for (name, block) in stations(&store) {
        let mut scene = station(&store, &mut gpu, block);
        let mut src = DatAssets::for_scene(&store, &scene.draw);
        let groups = resolved(&scene.draw);
        assert!(groups.len() > 20, "{name}: {} surface groups", groups.len());
        let mut pictures = BTreeSet::new();
        for (key, _, _) in &groups {
            let m = src.material(key, kind::Use::Static);
            *census.entry((m.data_category, m.kind)).or_insert(0) += 1;
            if let Some(t) = m.texture.clone() {
                if pictures.insert(t.clone()) {
                    let tex = src.texture(&t, m.clip_map);
                    assert!(tex.is_some(), "{name}: {t:?} does not decode");
                }
            }
        }
        let bake = &scene.draw.land.bake;
        assert_eq!(
            pictures.len(),
            usize::try_from(bake.textures_uploaded).unwrap(),
            "{name}: pictures, against Retail Look's surface-cache uploads"
        );
        let solid = pictures
            .iter()
            .filter(|t| matches!(t, TextureKey::Solid(_)))
            .count();
        assert_eq!(
            solid,
            usize::try_from(bake.solid_texels_uploaded).unwrap(),
            "{name}: one-texel colors"
        );
        scene.release_textures(&mut gpu);
    }
    eprintln!("(data category, kind) at the stations: {census:?}");
    for (category, _) in census.keys() {
        if let Some(c) = category {
            assert!(
                kind::BY_CATEGORY.iter().any(|(k, _)| k == c),
                "data category {c} has no kind of its own"
            );
        }
    }
}

/// **Every GfxObj Retail Look decoded at the stations is a mesh of the same triangles, surface by
/// surface**, and one with no drawing polygons is none.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn every_gfxobj_retail_look_decoded_is_a_mesh_of_the_same_triangles() {
    let store = store();
    let mut gpu = device();
    for (name, block) in stations(&store) {
        let mut scene = station(&store, &mut gpu, block);
        let mut src = DatAssets::for_scene(&store, &scene.draw);
        let geometry = &scene.draw.land.bake.geometry;
        assert!(geometry.len() > 50, "{name}: {} GfxObjs", geometry.len());
        for (id, groups) in geometry {
            let mesh = src.gfxobj(id.0);
            let Some(mesh) = mesh else {
                assert!(groups.is_empty(), "{name}: {id} has polygons and no mesh");
                continue;
            };
            assert_eq!(mesh.subsets.len(), groups.len(), "{name}: {id}");
            for (s, g) in mesh.subsets.iter().zip(groups) {
                assert_eq!(s.index_count as usize, g.vertices.len(), "{name}: {id}");
                assert_eq!(
                    (s.two_sided, s.tiled),
                    (g.two_sided, g.tiled),
                    "{name}: {id}"
                );
            }
        }
        scene.release_textures(&mut gpu);
    }
}

/// **A palette-swapped character's textures are as many as Retail Look makes for it**: the
/// body's parts, dressed with a shift palette and a palette range, built by Retail Look into a
/// surface cache of its own and by the remaster's tables, each from nothing.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_palette_swapped_characters_textures_are_as_many_as_retail_looks() {
    let store = store();
    let mut gpu = device();
    let mut scene = station(&store, &mut gpu, HOLTBURG);
    let region = dereth_world_data::landblock::load_region(&store).expect("the region decodes");
    let body = dereth_client_runtime::world_build::body_for(&store, &region, &scene.draw.cfg)
        .expect("the body builds");
    let mut array = body.driver().part_array.clone();
    // The base palette is the one the body's first palettised picture is drawn with; the range
    // lays the first palette set over its first 64 colors.
    let textures = TextureStore::new(&store);
    let base = array
        .parts
        .iter()
        .map(|p| p.gfxobj_id)
        .flat_map(|g| dereth_client_runtime::models::build_gfxobj(&store, g))
        .filter_map(|g| g.surface)
        .filter_map(|s| {
            let decoded = super::super::appearance::read_surface(&store, s)?;
            let (_, rs, _) = textures.resolve(decoded.orig_texture_id?).ok()?;
            rs.default_palette_id
        })
        .next()
        .expect("the body draws a palettised picture");
    let set = store
        .ids_of(DbType::PalSet)
        .into_iter()
        .next()
        .expect("the dat holds a palette set");
    let dress = dereth_animation::parts::ObjDesc {
        palette_id: base,
        subpalettes: vec![dereth_animation::parts::PaletteRange {
            palette_set: set,
            offset: 0,
            length: 8,
        }],
        ..dereth_animation::parts::ObjDesc::default()
    };
    assert!(array.do_obj_desc_changes(&dress), "the dress applies");

    // Retail Look, into a surface cache of its own.
    let (_, draw) = scene.halves_mut();
    let mut own = BakeCache::default();
    std::mem::swap(&mut draw.land.bake, &mut own);
    let built = draw.build_part_meshes(&store, &mut gpu, &array.parts, false);
    std::mem::swap(&mut draw.land.bake, &mut own);
    let built = built.expect("Retail Look builds the body");
    let retail = own.textures_uploaded;
    let shifted = own
        .surfaces
        .keys()
        .filter(|g| g.appearance.palette.is_some())
        .count();
    assert!(shifted > 0, "the dress shifts some surface");

    // The remaster, from nothing.
    let object = Object {
        key: ObjectKey::Body,
        setup: body.setup_id().0,
        parts: built
            .iter()
            .zip(&array.parts)
            .map(|(pl, part)| ObjectPart {
                levels: match &pl.info {
                    Some(info) => info.degrades.iter().map(|e| e.gfxobj_id.0).collect(),
                    None if pl.levels.first().is_some_and(|l| !l.is_empty()) => {
                        vec![pl.gfxobj.0]
                    }
                    None => Vec::new(),
                },
                look: part
                    .surface_overrides
                    .as_ref()
                    .map(|o| dereth_remaster::scene::PartLook {
                        textures: o.texture_maps.iter().map(|(a, b)| (a.0, b.0)).collect(),
                        palette: o.shift_palette.map(|p| p.0),
                        ranges: o
                            .subpalettes
                            .iter()
                            .map(|r| (r.palette_set.0, r.offset, r.length))
                            .collect(),
                    }),
            })
            .collect(),
    };
    // Retail Look's distinct pictures: a cached one by its key, a shifted one (which it uploads
    // once for each surface group drawing it) by the picture and the palette it is drawn with.
    let distinct: BTreeSet<(u64, Option<u32>, String)> = own
        .surfaces
        .iter()
        .filter(|(_, r)| r.texture.is_some())
        .map(|(g, r)| {
            let picture = g
                .appearance
                .texture
                .or(g.surface)
                .and_then(|t| textures.resolve(t).ok())
                .map(|x| x.0 .0);
            match (&g.appearance.palette, r.texture_key.is_uncached()) {
                (Some(p), true) => (0, picture, format!("{:?} {:?}", p.base, p.ranges)),
                _ => (r.texture_key.raw(), None, String::new()),
            }
        })
        .collect();
    let mut src = DatAssets::for_scene(&store, &scene.draw);
    let mut tables = AssetTables::new();
    tables.object_in(&object, &mut src);
    let c = tables.counts();
    assert_eq!(
        c.textures,
        distinct.len(),
        "the dressed body's pictures, against Retail Look's distinct ones"
    );
    // The surface groups are Retail Look's, the dress's appearance on exactly the surfaces it
    // changes.
    let body: BTreeSet<(MaterialKey, bool, bool)> = groups_of(&own).into_iter().collect();
    let ours = tables.surface_groups();
    assert_eq!(
        ours.symmetric_difference(&body).count(),
        0,
        "the dressed body's surface groups: only the remaster's {:?}; only Retail Look's {:?}",
        ours.difference(&body).collect::<Vec<_>>(),
        body.difference(&ours).collect::<Vec<_>>()
    );
    eprintln!(
        "a dressed body: {} pictures, which Retail Look uploads {retail} times across {} surface groups",
        c.textures,
        own.surfaces.len()
    );
    let ours_shifted = tables
        .texture_keys()
        .filter(|(k, _)| matches!(k, TextureKey::Shifted { .. }))
        .count();
    assert!(ours_shifted > 0, "the remaster's pictures are shifted too");
    for part in &built {
        for mesh in part.all() {
            if let Some(slot) = mesh.texture {
                own.release_group_texture(&mut gpu, slot);
            }
        }
    }
    scene.release_textures(&mut gpu);
}

/// Every change the scene publishes, kept.
#[derive(Debug, Default)]
struct Changes {
    deltas: Vec<dereth_remaster::scene::SceneDelta>,
    frames: usize,
}

impl dereth_remaster::scene::Receiver for Changes {
    fn receive(
        &mut self,
        deltas: Vec<dereth_remaster::scene::SceneDelta>,
        _frame: dereth_remaster::scene::FrameDelta,
    ) {
        self.deltas.extend(deltas);
        self.frames += 1;
    }
}

/// A sink that only counts what it is handed.
#[derive(Debug, Default)]
struct Counted {
    handed: BTreeMap<&'static str, usize>,
}

impl dereth_remaster::assets::AssetSink for Counted {
    fn texture(&mut self, _: &TextureKey, _: &TextureData) {
        *self.handed.entry("textures").or_insert(0) += 1;
    }
    fn material(&mut self, _: &MaterialKey, _: &dereth_remaster::assets::Material) {
        *self.handed.entry("materials").or_insert(0) += 1;
    }
    fn mesh(&mut self, _: dereth_remaster::assets::MeshKey, _: &dereth_remaster::assets::Mesh) {
        *self.handed.entry("meshes").or_insert(0) += 1;
    }
    fn terrain(
        &mut self,
        _: dereth_remaster::scene::BlockId,
        _: &dereth_remaster::assets::TerrainSurface,
    ) {
        *self.handed.entry("terrain").or_insert(0) += 1;
    }
    fn splat(&mut self, _: u32, _: &dereth_remaster::assets::terrain::Splat) {
        *self.handed.entry("splats").or_insert(0) += 1;
    }
    fn release(&mut self, _: &dereth_remaster::assets::AssetKey) {
        *self.handed.entry("released").or_insert(0) += 1;
    }
}

/// **At the Holtburg and forest stations, the remaster's tables, built from what the scene
/// publishes, hold what Retail Look holds**: a ground surface for every block Retail Look
/// meshed and a ground picture for every merge key it composited, every GfxObj it decoded and
/// no other, a piece for every room's geometry, a material for every surface group it resolved,
/// and a picture for every texture it uploaded; and an upload hands the device each of them
/// once.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn at_the_stations_the_tables_hold_what_retail_look_holds() {
    use dereth_client_runtime::present::Scene as _;
    let store = store();
    let mut gpu = device();
    for (name, block) in stations(&store) {
        let mut scene = station(&store, &mut gpu, block);
        let changes = std::rc::Rc::new(RefCell::new(Changes::default()));
        assert!(scene
            .draw
            .install_remaster_receiver(Box::new(std::rc::Rc::clone(&changes)))
            .is_none());
        gpu.begin_frame().expect("a frame begins");
        scene
            .draw
            .draw(&scene.world, &mut gpu)
            .expect("the frame draws");
        gpu.end_frame().expect("the frame ends");
        let changes = std::mem::take(&mut *changes.borrow_mut());
        assert_eq!(changes.frames, 1, "{name}: one frame published");
        let mut src = DatAssets::for_scene(&store, &scene.draw);
        let mut tables = AssetTables::new();
        tables.sky_in(&mut src);
        for d in &changes.deltas {
            tables.apply(d, &mut src);
        }
        let ours = tables.counts();
        let census = scene.census();
        let draw = &scene.draw;
        let bake = &draw.land.bake;
        eprintln!("{name}: {ours:?}\n  census {census:?}");

        assert_eq!(
            ours.terrain_blocks, census.blocks_meshed,
            "{name}: ground surfaces"
        );
        assert_eq!(
            ours.terrain_textures, census.terrain_surfaces,
            "{name}: ground pictures"
        );

        let meshes: BTreeSet<u32> = tables
            .mesh_keys()
            .filter_map(|(k, _)| match k {
                dereth_remaster::assets::MeshKey::GfxObj(id) => Some(id),
                dereth_remaster::assets::MeshKey::CellPiece(_)
                | dereth_remaster::assets::MeshKey::Sky(_) => None,
            })
            .collect();
        let decoded: BTreeSet<u32> = bake.geometry.keys().map(|k| k.0).collect();
        assert_eq!(
            meshes.symmetric_difference(&decoded).count(),
            0,
            "{name}: GfxObjs only the remaster holds: {:?}; only Retail Look: {:?}",
            meshes.difference(&decoded).collect::<Vec<_>>(),
            decoded.difference(&meshes).collect::<Vec<_>>()
        );

        let rooms: usize = draw.blocks.values().map(|b| b.env_cells.len()).sum();
        let pieces: BTreeSet<(u32, u16)> = changes
            .deltas
            .iter()
            .filter_map(|d| match d {
                dereth_remaster::scene::SceneDelta::BlockIn(b) => Some(b),
                _ => None,
            })
            .flat_map(|b| b.rooms.iter().map(|r| (r.environment, r.structure)))
            .collect();
        let sent_rooms: usize = changes
            .deltas
            .iter()
            .filter_map(|d| match d {
                dereth_remaster::scene::SceneDelta::BlockIn(b) => Some(b.rooms.len()),
                _ => None,
            })
            .sum();
        assert_eq!(sent_rooms, rooms, "{name}: rooms");
        assert_eq!(ours.cell_pieces, pieces.len(), "{name}: room pieces");

        let groups = tables.surface_groups();
        let retail: BTreeSet<(MaterialKey, bool, bool)> = resolved(draw).into_iter().collect();
        assert_eq!(
            groups.symmetric_difference(&retail).count(),
            0,
            "{name}: surface groups only the remaster holds: {:?}; only Retail Look: {:?}",
            groups.difference(&retail).collect::<Vec<_>>(),
            retail.difference(&groups).collect::<Vec<_>>()
        );
        assert_eq!(
            ours.surface_groups,
            bake.surfaces.len(),
            "{name}: surface groups"
        );
        assert_eq!(
            ours.textures - ours.terrain_textures,
            usize::try_from(bake.textures_uploaded).unwrap(),
            "{name}: pictures, against Retail Look's surface-cache uploads"
        );
        assert_eq!(
            ours.solid_textures,
            usize::try_from(bake.solid_texels_uploaded).unwrap(),
            "{name}: one-texel colors"
        );
        assert_eq!(ours.textures_missing, 0, "{name}: every picture decodes");

        let mut sink = Counted::default();
        tables.upload(&mut sink);
        assert_eq!(
            sink.handed.get("textures").copied().unwrap_or(0),
            ours.textures
        );
        assert_eq!(
            sink.handed.get("materials").copied().unwrap_or(0),
            ours.materials
        );
        assert_eq!(
            sink.handed.get("meshes").copied().unwrap_or(0),
            ours.meshes + ours.cell_pieces + ours.sky_objects
        );
        assert_eq!(
            sink.handed.get("terrain").copied().unwrap_or(0),
            ours.terrain_blocks
        );
        let u = tables.uploads();
        eprintln!("  uploaded {u:?}");

        // The ground as its splats: one a merge key without its detail level, every picture they
        // blend decoding, and no composite left.
        let keys: BTreeSet<u32> = changes
            .deltas
            .iter()
            .filter_map(|d| match d {
                dereth_remaster::scene::SceneDelta::BlockIn(b) => tables.terrain(b.block),
                _ => None,
            })
            .flat_map(|t| t.cells.iter())
            .filter_map(|c| match c.texture {
                TextureKey::Terrain(k) => Some(dereth_remaster::assets::terrain::splat_key(k)),
                _ => None,
            })
            .collect();
        tables.set_splat(true, &mut src);
        let split = tables.counts();
        eprintln!("  as splats {split:?}");
        assert_eq!(split.terrain_textures, 0, "{name}: no composite");
        assert_eq!(split.splats, keys.len(), "{name}: a splat a merge key");
        assert!(split.terrain_sources > 0);
        assert_eq!(split.textures_missing, 0, "{name}: every layer decodes");
        let mut sink = Counted::default();
        tables.upload(&mut sink);
        assert_eq!(
            sink.handed.get("splats").copied().unwrap_or(0),
            split.splats
        );
        scene.release_textures(&mut gpu);
    }
}

/// **A look keeps only what can change a surface's pixels**: a shift palette stays on a surface
/// whose picture is palettised and goes from one whose picture is block-compressed, and a texture
/// swap stays only on the surface whose own texture it names.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_look_keeps_only_what_can_change_a_surfaces_pixels() {
    let store = store();
    let textures = TextureStore::new(&store);
    let mut palettised = None;
    let mut compressed = None;
    for id in store.ids_of(DbType::Surface) {
        let Some(s) = super::super::appearance::read_surface(&store, id) else {
            continue;
        };
        let Some(t) = s.orig_texture_id else {
            continue;
        };
        match textures.is_palettised(t) {
            Ok(true) if palettised.is_none() => palettised = Some((id, t)),
            Ok(false) if compressed.is_none() => compressed = Some((id, t)),
            _ => {}
        }
        if palettised.is_some() && compressed.is_some() {
            break;
        }
    }
    let ((p, p_tex), (c, c_tex)) = (
        palettised.expect("a palettised surface"),
        compressed.expect("a block-compressed surface"),
    );
    let mut src = DatAssets::new(&store, 0, dereth_render::texture::ImageScale::FullRes);
    let dye = dereth_remaster::scene::PartLook {
        textures: vec![(c_tex.0, p_tex.0)],
        palette: Some(0x0400_007E),
        ranges: vec![(0x0F00_0001, 0, 8)],
    };
    let on_p = src.appearance(p.0, Some(&dye));
    assert_eq!(
        on_p.texture, None,
        "the swap names another surface's texture"
    );
    assert!(
        on_p.palette.is_some(),
        "a palettised picture takes the palette"
    );
    let on_c = src.appearance(c.0, Some(&dye));
    assert_eq!(
        on_c.texture,
        Some(p_tex.0),
        "the swap names this surface's texture"
    );
    assert!(
        on_c.palette.is_some(),
        "and the picture swapped in is palettised, so the palette stays"
    );
    let plain = dereth_remaster::scene::PartLook {
        textures: Vec::new(),
        ..dye
    };
    let on_c = src.appearance(c.0, Some(&plain));
    assert_eq!(
        on_c,
        dereth_remaster::assets::Appearance::default(),
        "a palette cannot change a block-compressed picture"
    );
    assert_eq!(
        src.appearance(p.0, None),
        dereth_remaster::assets::Appearance::default()
    );
}
