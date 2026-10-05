//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The February 2005 region's palette-shift land surface covers the whole world: every cell of
//! every landblock finds a texture with a palette for each of its corners and a rotation whose road
//! pattern the texture lists, and every palette it names exists.
//! Fixture: the February 2005 portal and cell files (`DERETH_TEST_PRETOD_DAT_DIR`).

use std::collections::BTreeSet;

use dereth_assets::world::CellLandblock;
use dereth_assets::{decode_any_in, Decode, DecodedAsset};
use dereth_dat::{ContainerEra, DbType};
use dereth_primitives::DataId;
use {
    dereth_terrain::land::merge::cell_rotation_keys, dereth_terrain::land::merge::cell_x,
    dereth_terrain::land::merge::cell_y, dereth_terrain::land::merge::is_pal_shifted,
};
use {
    dereth_terrain::land::palshift::road_pattern, dereth_terrain::land::palshift::select,
    dereth_terrain::land::palshift::sub_palettes,
};

#[test]
fn every_february_2005_cell_finds_a_palette_shift_texture_and_rotation() {
    let s = dereth_dat::testing::open_pre_tod_store_or_fail();
    let id = DataId(0x1300_0000);
    let bytes = s.read_typed(DbType::Region, id).expect("the region");
    let DecodedAsset::Region(region) =
        decode_any_in(ContainerEra::PreTod, DbType::Region, id, &bytes).expect("decodes")
    else {
        panic!("not a region");
    };
    assert!(is_pal_shifted(&region));
    let ps = region
        .land_surf
        .pal_shift
        .as_ref()
        .expect("palette shifting");
    assert_eq!(ps.textures.len(), 2);

    let mut palettes = BTreeSet::new();
    let (mut cells, mut rotated) = (0usize, 0usize);
    for lb_id in s.ids_of(DbType::LandBlock) {
        let bytes = s.read_cell(lb_id).expect("reads");
        let lb =
            CellLandblock::decode_payload_in(ContainerEra::PreTod, lb_id, &bytes).expect("decodes");
        for i in 0..8 {
            for j in 0..8 {
                let (keys, _) = cell_rotation_keys(&lb, &region, 8, i, j);
                let c = select(ps, &keys, cell_x(&lb, i), cell_y(&lb, j));
                let texture = &ps.textures[c.texture];
                assert!(
                    texture
                        .road_maps
                        .get(c.road)
                        .is_some_and(|r| r.road_code == road_pattern(c.key)),
                    "{lb_id} cell ({i},{j}): no texture lists its road pattern"
                );
                assert_eq!(keys[c.rotation.index()], c.key);
                for (p, _, _) in sub_palettes(ps, &c) {
                    palettes.insert(p);
                }
                cells += 1;
                rotated += usize::from(c.rotation.index() != 0);
            }
        }
    }
    assert_eq!(cells, 65_025 * 64);
    assert!(rotated > 0, "some cells are drawn rotated");
    for p in palettes {
        assert!(s.portal().contains(p), "palette {p} is in the portal file");
    }
}

/// The February 2005 cells read as the end-of-retail region numbers its terrains, so the later
/// region's texture-merge land surface can draw them: every terrain type the older region names
/// has the same name and map colour at the same index in the later region (which adds one at the
/// end), and every terrain type the older cells use has a texture-merge descriptor in the later
/// region, as the road does.
#[test]
fn the_february_2005_cells_read_as_the_later_region_numbers_its_terrains() {
    let old = dereth_dat::testing::open_pre_tod_store_or_fail();
    let new = dereth_dat::testing::open_store_or_fail();
    let id = DataId(0x1300_0000);
    let DecodedAsset::Region(older) = decode_any_in(
        ContainerEra::PreTod,
        DbType::Region,
        id,
        &old.read_typed(DbType::Region, id)
            .expect("the older region"),
    )
    .expect("decodes") else {
        panic!("not a region");
    };
    let later = dereth_assets::region::Region::decode_payload(
        id,
        &new.read_typed(DbType::Region, id)
            .expect("the later region"),
    )
    .expect("decodes");
    assert!(later.terrain_types.len() >= older.terrain_types.len());
    for (i, (a, b)) in older
        .terrain_types
        .iter()
        .zip(&later.terrain_types)
        .enumerate()
    {
        assert_eq!(
            (&a.terrain_name, a.terrain_color),
            (&b.terrain_name, b.terrain_color),
            "terrain type {i}"
        );
    }
    let tm = later
        .land_surf
        .tex_merge
        .as_ref()
        .expect("the later region texture-merges");
    let described: BTreeSet<u32> = tm.terrain_desc.iter().map(|d| d.terrain_type).collect();
    assert!(
        described.contains(&dereth_terrain::consts::ROAD_TERRAIN_TYPE),
        "the road"
    );
    let mut used = BTreeSet::new();
    for lb_id in old.ids_of(DbType::LandBlock) {
        let bytes = old.read_cell(lb_id).expect("reads");
        let lb =
            CellLandblock::decode_payload_in(ContainerEra::PreTod, lb_id, &bytes).expect("decodes");
        used.extend(lb.terrain.iter().map(|w| u32::from((w >> 2) & 0x1F)));
    }
    let missing: Vec<u32> = used.difference(&described).copied().collect();
    assert!(missing.is_empty(), "no later descriptor for {missing:?}");
    assert!(
        used.iter()
            .all(|&t| (t as usize) < older.terrain_types.len()),
        "the older cells use a terrain type their own region does not name: {used:?}"
    );
}
