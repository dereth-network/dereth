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
use dereth_world_render::land::merge::{cell_rotation_keys, cell_x, cell_y, is_pal_shifted};
use dereth_world_render::land::palshift::{road_pattern, select, sub_palettes};

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
