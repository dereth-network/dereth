//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Height table lookup, vertices at every LOD, seam continuity, land limits, reduced-LOD subsets,
//! texmerge region, alpha masks, merge-key rotations, the forbidden mask never emitted, water
//! classification, vertex lighting, scenery filters and determinism, terrain words vs scene types
//! over the whole world.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use dereth_assets::region::Region;
use dereth_assets::world::{CellLandblock, LandblockInfo, Scene};
use dereth_assets::{decode_any, DecodedAsset};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, Vec3};
use dereth_terrain::land::water::WaterType;
use dereth_terrain::scenery::{generate_scenery, SceneryEnv, WithinBlockShape};
use {
    dereth_terrain::consts::LAND_HEIGHT_TABLE_LEN, dereth_terrain::consts::SIDE_VERTEX_COUNT,
    dereth_terrain::consts::VERTEX_COUNT,
};
use {
    dereth_terrain::land::lighting::calc_lighting,
    dereth_terrain::land::lighting::LandscapeLighting,
};
use {
    dereth_terrain::land::merge::alpha_map_index, dereth_terrain::land::merge::cell_rotation_keys,
    dereth_terrain::land::merge::find_road_alpha, dereth_terrain::land::merge::find_terrain_alpha,
    dereth_terrain::land::merge::get_road_code, dereth_terrain::land::merge::get_terrain,
    dereth_terrain::land::merge::get_terrain_tex, dereth_terrain::land::merge::MergeKey,
};
use {
    dereth_terrain::land::mesh::generate_landblock_with_table,
    dereth_terrain::land::mesh::height_table, dereth_terrain::land::mesh::sw_to_ne_cut,
    dereth_terrain::land::mesh::Direction,
};

const REGION_ID: DataId = DataId(0x1300_0000);

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

fn region(s: &RetailDatStore) -> Region {
    let b = s
        .read_typed(DbType::Region, REGION_ID)
        .expect("the region record");
    match decode_any(DbType::Region, REGION_ID, &b).expect("region decodes") {
        DecodedAsset::Region(r) => r,
        other => panic!("0x13000000 decoded as {other:?}"),
    }
}

fn landblock(s: &RetailDatStore, id: DataId) -> CellLandblock {
    let b = s
        .read_typed(DbType::LandBlock, id)
        .expect("a landblock record");
    match decode_any(DbType::LandBlock, id, &b).expect("landblock decodes") {
        DecodedAsset::Landblock(l) => l,
        other => panic!("{id} decoded as {other:?}"),
    }
}

fn block_xy(id: DataId) -> (i32, i32) {
    // LINT-OK: both bytes of the landblock id, each 0..=0xFE.
    (((id.0 >> 24) & 0xFF) as i32, ((id.0 >> 16) & 0xFF) as i32)
}

#[test]
fn the_height_table_is_a_lookup_and_stops_being_twice_the_index_at_201() {
    let s = store();
    let r = region(&s);
    let t = height_table(&r);
    assert_eq!(t.len(), LAND_HEIGHT_TABLE_LEN);
    // The linear prefix.
    let linear = (0..256)
        .take_while(|&i| {
            #[allow(clippy::cast_precision_loss)]
            let expect = 2.0 * i as f32;
            (t[i] - expect).abs() < 1e-6
        })
        .count();
    assert_eq!(
        linear, 201,
        "the retail table is 2*i for exactly entries 0..=200"
    );
    assert_eq!(t[200], 400.0);
    assert!(t[201] > 402.0, "entry 201 leaves the 2*i line: {}", t[201]);
    assert_eq!(t[255], 700.0, "the table tops out at 700 metres");
    // Monotone, and inside the [0, 800] window `set_height_table` validates against.
    for i in 1..256 {
        assert!(t[i] >= t[i - 1], "entry {i} goes backwards");
        assert!(
            (0.0..=800.0).contains(&t[i]),
            "entry {i} is outside [0, 800]"
        );
    }
    // Doubling the byte would misplace every high vertex by up to 190 metres.
    #[allow(clippy::cast_precision_loss)]
    let worst = (0..256)
        .map(|i| (t[i] - 2.0 * i as f32).abs())
        .fold(0.0f32, f32::max);
    assert!(
        worst > 100.0,
        "the doubling error is large and visible: {worst}"
    );
}

#[test]
fn every_landblock_builds_vertices_from_the_height_table_at_every_lod() {
    let s = store();
    let r = region(&s);
    let t = height_table(&r);
    let ids = s.ids_of(DbType::LandBlock);
    assert!(!ids.is_empty(), "landblocks are exercised");
    let input_count = ids.len() as u64;

    let mut vertices_checked = 0u64;
    let mut polygons_checked = 0u64;
    let mut cuts_true = 0u64;
    let mut cuts_total = 0u64;
    for id in &ids {
        let lb = landblock(&s, *id);
        let (bx, by) = block_xy(*id);
        let m = generate_landblock_with_table(&lb, &r, &t, bx, by, 1, Direction::InViewerBlock);
        assert_eq!(m.side_cell_count, 8);
        assert_eq!(m.vertices.len(), 81);
        assert_eq!(m.polygons.len(), 128, "two triangles per cell");
        assert_eq!(m.sw_to_ne_cut.len(), 64);
        for i in 0..9usize {
            for j in 0..9usize {
                let v = m.vertices[i * 9 + j];
                #[allow(clippy::cast_precision_loss)]
                {
                    assert_eq!(v.x, i as f32 * 24.0);
                    assert_eq!(v.y, j as f32 * 24.0);
                }
                assert_eq!(
                    v.z,
                    t[lb.height[i * SIDE_VERTEX_COUNT + j] as usize],
                    "{id} ({i},{j})"
                );
                vertices_checked += 1;
            }
        }
        for p in &m.polygons {
            // Every terrain plane faces up, which is what the back-face test relies on.
            assert!(
                p.plane.normal.z > 0.0,
                "{id} has a downward-facing terrain triangle"
            );
            // The plane really passes through its three vertices.
            for &vi in &p.v {
                let d = p.plane.dot_point(m.vertices[vi as usize]);
                assert!(d.abs() < 0.02, "{id}: vertex {vi} is {d} off its own plane");
            }
            polygons_checked += 1;
        }
        cuts_true += m.sw_to_ne_cut.iter().filter(|&&c| c).count() as u64;
        cuts_total += m.sw_to_ne_cut.len() as u64;
    }
    assert_eq!(vertices_checked, input_count * 81, "5 267 025 vertices");
    assert_eq!(polygons_checked, input_count * 128, "8 323 200 triangles");
    assert_eq!(cuts_total, input_count * 64, "4 161 600 split flags");
    let ratio = cuts_true as f64 / cuts_total as f64;
    assert!(
        (0.45..0.55).contains(&ratio),
        "split ratio {ratio} looks biased"
    );
}

/// Oracle: the client's own block-edge continuity rule (mechanism 2 in
/// `11-landscape-rendering.md`) — the split hash is over **global** cell coordinates, so the cell
/// on one block's east edge and the same cell reached from its neighbour's west edge must get the
/// same diagonal. Any block-local term in the hash breaks every landblock seam in Dereth, and this
/// checks all 254 x 255 shared edges in both axes.
#[test]
fn the_diagonal_is_continuous_across_every_landblock_seam() {
    for bx in 0..254i32 {
        for by in 0..255i32 {
            for j in 0..8i32 {
                let a = sw_to_ne_cut(bx * 8 + 8, by * 8 + j);
                let b = sw_to_ne_cut((bx + 1) * 8, by * 8 + j);
                assert_eq!(a, b, "east seam of block ({bx},{by}) row {j}");
            }
        }
    }
    for bx in 0..255i32 {
        for by in 0..254i32 {
            for i in 0..8i32 {
                let a = sw_to_ne_cut(bx * 8 + i, by * 8 + 8);
                let b = sw_to_ne_cut(bx * 8 + i, (by + 1) * 8);
                assert_eq!(a, b, "north seam of block ({bx},{by}) column {i}");
            }
        }
    }
}

/// Oracle: — `max_zval` is the tallest terrain vertex plus
/// 200 (room for buildings) and `min_zval` the lowest minus 1, over the **full 9x9 array**
/// regardless of LOD. Checked against the retail height bytes on a spread of landblocks.
#[test]
fn land_limits_bracket_every_vertex_with_the_documented_margins() {
    let s = store();
    let r = region(&s);
    let t = height_table(&r);
    for id in s.ids_of(DbType::LandBlock).into_iter().step_by(997) {
        let lb = landblock(&s, id);
        let (bx, by) = block_xy(id);
        for lod in [1u8, 2, 4, 8] {
            let m =
                generate_landblock_with_table(&lb, &r, &t, bx, by, lod, Direction::InViewerBlock);
            let hi = t[lb.height.iter().copied().max().expect("81 entries") as usize];
            let lo = t[lb.height.iter().copied().min().expect("81 entries") as usize];
            assert_eq!(m.max_zval, hi + 200.0, "{id} lod {lod}");
            assert_eq!(m.min_zval, lo - 1.0, "{id} lod {lod}");
            for v in &m.vertices {
                assert!(
                    v.z <= m.max_zval && v.z >= m.min_zval,
                    "{id}: vertex escapes the column"
                );
            }
        }
    }
}

/// Oracle: §"" and §"LOD rings" — `side_cell_count = 8 / lod_div`,
/// vertices are `(side_cell_count + 1)²` and are sampled at `step = 8 / side_cell_count`, so a
/// reduced-detail block's vertices are a **subset** of the full-detail ones.
#[test]
fn reduced_detail_blocks_sample_a_subset_of_the_full_detail_vertices() {
    let s = store();
    let r = region(&s);
    let t = height_table(&r);
    for id in s.ids_of(DbType::LandBlock).into_iter().step_by(4001) {
        let lb = landblock(&s, id);
        let (bx, by) = block_xy(id);
        let full = generate_landblock_with_table(&lb, &r, &t, bx, by, 1, Direction::InViewerBlock);
        for (lod, n) in [(2u8, 4usize), (4, 2), (8, 1)] {
            let m =
                generate_landblock_with_table(&lb, &r, &t, bx, by, lod, Direction::InViewerBlock);
            assert_eq!(usize::from(m.side_cell_count), n, "lod {lod}");
            assert_eq!(m.vertices.len(), (n + 1) * (n + 1));
            assert_eq!(m.polygons.len(), 2 * n * n);
            let step = 8 / n;
            for i in 0..=n {
                for j in 0..=n {
                    let coarse = m.vertices[i * (n + 1) + j];
                    let fine = full.vertices[(i * step) * 9 + (j * step)];
                    assert_eq!(coarse, fine, "{id} lod {lod} ({i},{j})");
                }
            }
        }
    }
}

/// The retail region uses texmerge with a 1024 base texture.
#[test]
fn the_retail_region_uses_texmerge_with_a_1024_base_texture() {
    let s = store();
    let r = region(&s);
    assert_eq!(
        r.land_surf.surf_type, 0,
        "open question #78: the land-surface type is 0 on retail data"
    );
    let tm = r
        .land_surf
        .tex_merge
        .as_ref()
        .expect("a terrain texture-merge record");
    assert_eq!(
        tm.base_tex_size, 1024,
        "open question #79: base_tex_size is 1024"
    );
    assert_eq!(
        tm.terrain_desc.len(),
        33,
        "32 terrain types plus the road entry at type 32"
    );
    assert!(
        get_terrain_tex(tm, 0x20).is_some_and(|d| d.terrain_type == 0x20),
        "GetTerrainTex(0x20) must find the road entry, not fall back to entry 0"
    );
}

#[test]
fn every_producible_alpha_mask_resolves_in_the_retail_tables() {
    let s = store();
    let r = region(&s);
    let tm = r
        .land_surf
        .tex_merge
        .as_ref()
        .expect("a terrain texture-merge record");
    assert_eq!(
        tm.corner_terrain_maps
            .iter()
            .map(|c| c.code)
            .collect::<Vec<_>>(),
        vec![8, 8, 8, 8],
        "the retail corner maps are all tcode 8"
    );
    assert_eq!(
        tm.side_terrain_maps
            .iter()
            .map(|c| c.code)
            .collect::<Vec<_>>(),
        vec![9]
    );
    assert_eq!(
        tm.road_maps.iter().map(|c| c.code).collect::<Vec<_>>(),
        vec![9, 10, 8]
    );

    for code in [1u32, 2, 4, 8, 3, 6, 12] {
        let k = MergeKey(code.wrapping_mul(0x9E37_79B9)); // an arbitrary key to drive the index hash
        assert!(
            find_terrain_alpha(tm, code, k).is_some(),
            "terrain mask {code} has no alpha map"
        );
    }
    let mut road_masks = BTreeSet::new();
    for rc in 0..16u32 {
        let roads = [
            u16::from(rc & 1 != 0),
            u16::from(rc & 2 != 0),
            u16::from(rc & 4 != 0),
            u16::from(rc & 8 != 0),
        ];
        let g = get_road_code(MergeKey::new([0; 4], roads, 1));
        for c in g.codes {
            if c != 0 {
                road_masks.insert(c);
            }
        }
    }
    assert_eq!(
        road_masks.iter().copied().collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6, 8, 9, 10, 12],
        "the ten masks GetRoadCode can produce"
    );
    for code in road_masks {
        let k = MergeKey(code.wrapping_mul(0x85EB_CA6B));
        assert!(
            find_road_alpha(tm, code, k).is_some(),
            "road mask {code} has no alpha map"
        );
    }
    // The road table needs the scan: mask 10 lives in an orbit only road_maps[1] reaches, and the
    // index hash does not always land there.
    let mut needed_scan = 0;
    for n in 0..64u32 {
        let k = MergeKey(n.wrapping_mul(0xC2B2_AE35));
        if alpha_map_index(k, 3) != 1 && find_road_alpha(tm, 10, k).is_some() {
            needed_scan += 1;
        }
    }
    assert!(needed_scan > 0, "FindRoadAlpha must scan, not index");
}

/// Merge keys for every retail cell are four rotations of one pattern.
#[test]
fn merge_keys_for_every_retail_cell_are_four_rotations_of_one_pattern() {
    let s = store();
    let r = region(&s);
    let ids = s.ids_of(DbType::LandBlock);
    let mut unique: BTreeSet<MergeKey> = BTreeSet::new();
    let mut cells = 0u64;
    let mut uniform_cells = 0u64;
    for id in ids.iter().step_by(7) {
        let lb = landblock(&s, *id);
        for i in 0..8usize {
            for j in 0..8usize {
                let (keys, uniform) = cell_rotation_keys(&lb, &r, 8, i, j);
                // Each rotation is the corner list shifted by one, so the type fields rotate.
                let t0 = keys[0].corner_types();
                for k in 1..4usize {
                    let tk = keys[k].corner_types();
                    for c in 0..4usize {
                        assert_eq!(tk[c], t0[(c + k) % 4], "{id} cell ({i},{j}) rot {k}");
                    }
                    assert_eq!(keys[k].pal_lod(), 1);
                }
                // uniform <=> all four corners identical and no road.
                let same = t0.iter().all(|&t| t == t0[0]);
                let no_road = (keys[0].0 >> 20) & 0xFF == 0;
                assert_eq!(uniform, same && no_road, "{id} cell ({i},{j})");
                uniform_cells += u64::from(uniform);
                unique.insert(keys[0]);
                cells += 1;
            }
        }
        // A reduced-detail block gets palLod 4, which is what makes its merged texture smaller.
        let (keys4, _) = cell_rotation_keys(&lb, &r, 4, 0, 0);
        assert_eq!(keys4[0].pal_lod(), 4, "{id} at side_cell_count 4");
    }
    assert!(cells > 0, "checked {cells} cells");
    assert!(
        unique.len() > 0,
        "the world uses {} distinct merge keys",
        unique.len()
    );
    assert!(
        uniform_cells > 0,
        "some cells must be uniform, or the stippling flag is dead"
    );
}

#[test]
fn get_terrain_over_the_whole_world_never_emits_the_forbidden_mask() {
    let s = store();
    let r = region(&s);
    let mut seen_codes: BTreeMap<u32, u64> = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for id in s.ids_of(DbType::LandBlock).into_iter().step_by(11) {
        let lb = landblock(&s, id);
        for i in 0..8usize {
            for j in 0..8usize {
                let (k, _, _) = dereth_terrain::land::merge::cell_rotation(&lb, &r, 8, i, j);
                keys.insert(k);
            }
        }
    }
    for &k in &keys {
        let g = get_terrain(k);
        let t = k.corner_types();
        assert!(t.contains(&g.tex[0].expect("a base terrain type")), "{k:?}");
        for c in g.code {
            if c == 0 {
                continue;
            }
            assert!(
                matches!(c, 1 | 2 | 4 | 8 | 3 | 6 | 12),
                "{k:?} emitted mask {c}, which GetTerrain cannot produce"
            );
            assert_ne!(
                c, 9,
                "mask 9 (NW, SW) is never produced -- see the GetTerrain asymmetry"
            );
            *seen_codes.entry(c).or_default() += 1;
        }
    }
    assert!(keys.len() > 0, "{} distinct keys", keys.len());
    // The world exercises both corner and side masks; if only one kind appeared the two alpha
    // tables would never both be reached.
    assert!(
        seen_codes.keys().any(|c| matches!(c, 1 | 2 | 4 | 8)),
        "no corner overlays: {seen_codes:?}"
    );
    assert!(
        seen_codes.keys().any(|c| matches!(c, 3 | 6 | 12)),
        "no side overlays: {seen_codes:?}"
    );
}

#[test]
fn water_classification_over_the_whole_world_finds_all_three_kinds() {
    let s = store();
    let r = region(&s);
    let t = height_table(&r);
    let mut counts = [0u64; 3];
    for id in s.ids_of(DbType::LandBlock) {
        let lb = landblock(&s, id);
        let (bx, by) = block_xy(id);
        let m = generate_landblock_with_table(&lb, &r, &t, bx, by, 1, Direction::InViewerBlock);
        counts[m.water_type as usize] += 1;
        assert_eq!(m.cell_water.len(), 64);
        // The block summary must agree with its own cells.
        let any = m.cell_water.iter().any(|&w| w != WaterType::NotWater);
        let all = m.cell_water.iter().all(|&w| w == WaterType::EntirelyWater);
        let expect = if !any {
            WaterType::NotWater
        } else if all {
            WaterType::EntirelyWater
        } else {
            WaterType::PartiallyWater
        };
        assert_eq!(m.water_type, expect, "{id}");
        for lod in [2u8, 4, 8] {
            let l =
                generate_landblock_with_table(&lb, &r, &t, bx, by, lod, Direction::InViewerBlock);
            assert!(l.cell_water.is_empty(), "{id} lod {lod}");
            assert_eq!(l.water_type, WaterType::NotWater, "{id} lod {lod}");
        }
    }
    assert_eq!(
        counts.iter().sum::<u64>(),
        s.ids_of(DbType::LandBlock).len() as u64
    );
    assert!(counts[0] > 0, "some blocks are dry");
    assert!(counts[1] > 0, "some blocks are partially water: {counts:?}");
    assert!(counts[2] > 0, "some blocks are entirely water: {counts:?}");
}

#[test]
fn terrain_vertex_lighting_tracks_the_sun_over_retail_terrain() {
    let s = store();
    let r = region(&s);
    let t = height_table(&r);
    let suns = [
        Vec3::new(1.2, 0.0, 0.9),
        Vec3::new(0.6, 0.0, 0.45),
        Vec3::new(0.1, 0.0, 0.075),
    ];
    let mut checked = 0u64;
    for id in s.ids_of(DbType::LandBlock).into_iter().step_by(499) {
        let lb = landblock(&s, id);
        let (bx, by) = block_xy(id);
        let m = generate_landblock_with_table(&lb, &r, &t, bx, by, 1, Direction::InViewerBlock);
        let mut prev: Option<Vec<[u8; 3]>> = None;
        for sun in suns {
            let l = LandscapeLighting {
                ambient_level: 0.0,
                ambient_color: [0, 0, 0],
                sunlight: sun,
                sunlight_color: [255, 255, 255],
            };
            let c = calc_lighting(&m, &l);
            assert_eq!(c.len(), 81);
            for px in &c {
                assert_eq!(px[0], px[1], "a white sun lights all channels equally");
                assert_eq!(px[1], px[2]);
            }
            if let Some(p) = prev {
                for (a, b) in p.iter().zip(&c) {
                    assert!(b[0] <= a[0], "{id}: a dimmer sun brightened a vertex");
                }
            }
            prev = Some(c);
            checked += 81;
        }
    }
    assert!(checked > 0, "{checked} vertex colours");
}

#[test]
fn scenery_over_retail_landblocks_satisfies_every_filter_and_is_deterministic() {
    let s = store();
    let r = region(&s);
    let t = height_table(&r);
    // Cache decoded scenes; the same handful is used by the whole world.
    let cache: RefCell<BTreeMap<u32, Option<Scene>>> = RefCell::new(BTreeMap::new());
    let load = |id: DataId| -> Option<Scene> {
        cache
            .borrow_mut()
            .entry(id.0)
            .or_insert_with(|| {
                let b = s.read_typed(DbType::Scene, id).ok()?;
                match decode_any(DbType::Scene, id, &b).ok()? {
                    DecodedAsset::Scene(sc) => Some(sc),
                    _ => None,
                }
            })
            .clone()
    };

    let ids: Vec<DataId> = s
        .ids_of(DbType::LandBlock)
        .into_iter()
        .step_by(1301)
        .collect();
    assert!(!ids.is_empty(), "{} landblocks in the sample", ids.len());
    let mut placed_total = 0usize;
    let mut blocks_with_scenery = 0usize;
    for id in ids {
        let lb = landblock(&s, id);
        let (bx, by) = block_xy(id);
        let m = generate_landblock_with_table(&lb, &r, &t, bx, by, 1, Direction::InViewerBlock);

        // Buildings come from the landblock-info record, when there is one.
        let mut building_cells = BTreeSet::new();
        if lb.lbi_exists != 0 {
            let info_id = DataId((id.0 & 0xFFFF_0000) | 0xFFFE);
            if let Ok(b) = s.read_typed(DbType::Lbi, info_id) {
                if let Ok(DecodedAsset::LandblockInfo(li)) = decode_any(DbType::Lbi, info_id, &b) {
                    let li: LandblockInfo = li;
                    for bl in &li.buildings {
                        // A building's frame origin names the cell it stands in.
                        building_cells.insert(dereth_terrain::scenery::outside_cell_index(
                            bl.frame.origin.x,
                            bl.frame.origin.y,
                        ));
                    }
                }
            }
        }
        let has_building = |c: u16| building_cells.contains(&c);
        let no_shape = |_: DataId| Some(WithinBlockShape::default());
        let scenes_fn = |d: DataId| load(d);
        let env = SceneryEnv {
            scenes: &scenes_fn,
            has_building: &has_building,
            shape: &no_shape,
        };
        let placed = generate_scenery(&lb, &m, &r, bx, by, &env);
        // Determinism: the same inputs must give the same objects, in the same order.
        let again = generate_scenery(&lb, &m, &r, bx, by, &env);
        assert_eq!(placed, again, "{id}: scenery is not deterministic");

        for p in &placed {
            assert!(
                (0.0..192.0).contains(&p.frame.origin.x),
                "{id}: {p:?} escapes the block"
            );
            assert!(
                (0.0..192.0).contains(&p.frame.origin.y),
                "{id}: {p:?} escapes the block"
            );
            assert!(
                !dereth_terrain::road::on_road(&lb.terrain, p.frame.origin.x, p.frame.origin.y),
                "{id}: {p:?} stands on a road"
            );
            let ci = p.cell.index();
            assert!((1..=64).contains(&ci), "{id}: {p:?} is not in a land cell");
            assert!(
                !has_building(ci),
                "{id}: {p:?} is in a cell that owns a building"
            );
            // The object sits on the terrain, not floating: within the block's z band.
            assert!(
                p.frame.origin.z >= m.min_zval && p.frame.origin.z <= m.max_zval,
                "{id}: {p:?} is outside the block's z column"
            );
            assert!(p.scale > 0.0, "{id}: {p:?} has a non-positive scale");
            assert_ne!(
                p.gfxobj.0, 0,
                "{id}: gfxobj_id 0 means draw nothing and must not be placed"
            );
        }
        for lod in [2u8, 4, 8] {
            let lm =
                generate_landblock_with_table(&lb, &r, &t, bx, by, lod, Direction::InViewerBlock);
            assert!(
                generate_scenery(&lb, &lm, &r, bx, by, &env).is_empty(),
                "{id} lod {lod} grew scenery"
            );
        }
        placed_total += placed.len();
        blocks_with_scenery += usize::from(!placed.is_empty());
    }
    assert!(
        blocks_with_scenery > 0,
        "no landblock in the sample grew any scenery"
    );
    assert!(
        placed_total > 0,
        "only {placed_total} objects placed across the sample"
    );
}

/// Scenery is a function of global cell coordinates.
#[test]
fn scenery_is_a_function_of_global_cell_coordinates() {
    let s = store();
    let r = region(&s);
    let t = height_table(&r);
    let cache: RefCell<BTreeMap<u32, Option<Scene>>> = RefCell::new(BTreeMap::new());
    let load = |id: DataId| -> Option<Scene> {
        cache
            .borrow_mut()
            .entry(id.0)
            .or_insert_with(|| {
                let b = s.read_typed(DbType::Scene, id).ok()?;
                match decode_any(DbType::Scene, id, &b).ok()? {
                    DecodedAsset::Scene(sc) => Some(sc),
                    _ => None,
                }
            })
            .clone()
    };
    // Find a landblock that actually grows scenery.
    let mut chosen = None;
    for id in s.ids_of(DbType::LandBlock).into_iter().step_by(97) {
        let lb = landblock(&s, id);
        let (bx, by) = block_xy(id);
        let m = generate_landblock_with_table(&lb, &r, &t, bx, by, 1, Direction::InViewerBlock);
        let scenes_fn = |d: DataId| load(d);
        let env = SceneryEnv {
            scenes: &scenes_fn,
            has_building: &|_| false,
            shape: &|_| Some(WithinBlockShape::default()),
        };
        if generate_scenery(&lb, &m, &r, bx, by, &env).len() > 5 {
            chosen = Some((id, lb, m, bx, by));
            break;
        }
    }
    let (id, lb, m, bx, by) = chosen.expect("some retail landblock grows scenery");
    let scenes_fn = |d: DataId| load(d);
    let env = SceneryEnv {
        scenes: &scenes_fn,
        has_building: &|_| false,
        shape: &|_| Some(WithinBlockShape::default()),
    };
    let here = generate_scenery(&lb, &m, &r, bx, by, &env);
    let elsewhere = generate_scenery(&lb, &m, &r, bx + 1, by, &env);
    assert_ne!(
        here, elsewhere,
        "{id}: the same terrain one block east grew identical scenery"
    );
    assert_eq!(
        here,
        generate_scenery(&lb, &m, &r, bx, by, &env),
        "{id}: not reproducible"
    );
}

#[test]
fn the_shipped_terrain_words_never_name_a_scene_type_the_region_lacks() {
    let s = store();
    let r = region(&s);
    assert_eq!(r.terrain_types.len(), 32);
    for (i, t) in r.terrain_types.iter().enumerate() {
        assert_eq!(t.scene_types.len(), 32, "terrain type {i}");
    }
    let mut out_of_range = 0u64;
    let mut words = 0u64;
    for id in s.ids_of(DbType::LandBlock).into_iter().step_by(13) {
        let lb = landblock(&s, id);
        for w in lb.terrain {
            let tt = usize::from((w >> 2) & 0x1F);
            let st = usize::from(w >> 11);
            words += 1;
            if st >= dereth_terrain::scenery::num_scene_type(&r, tt) {
                out_of_range += 1;
            }
            assert_eq!(
                w & 0x0780,
                0,
                "{id}: terrain word {w:#06X} has bits 0x0780 set"
            );
        }
    }
    assert!(words > 0, "{words} terrain words");
    assert_eq!(
        out_of_range, 0,
        "{out_of_range} vertices name a scene type the region lacks"
    );
    assert_eq!(VERTEX_COUNT, 81);
}
