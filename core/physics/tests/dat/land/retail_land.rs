//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The retail height table passes client validation; every landblock builds and matches the dat;
//! edges agree across the world; the split hash is seam-independent; entirely-water blocks exist;
//! only terrain 16-20 is water; the seam test discriminates axes.
//! Fixture: the shipped retail DAT records and recorded inputs.

use dereth_assets::{CellLandblock, Decode, Region};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::land::{
    calc_cell_water, split_hash, surf_char, LandblockCollision, SurfChar, WaterType,
    SIDE_VERTEX_COUNT,
};
use dereth_physics::landdefs;
use dereth_primitives::{DataId, LandblockId};

/// The shipped region whose land definitions are checked below.
const REGION_ID: DataId = DataId(0x1300_0000);

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

fn height_table(store: &RetailDatStore) -> [f32; 256] {
    let bytes = store
        .read_typed(DbType::Region, REGION_ID)
        .expect("region 0x13000000");
    let region = Region::decode_payload(REGION_ID, &bytes).expect("region decodes");
    landdefs::validate_height_table(&region.land_defs.land_height_table)
        .expect(" accepts the retail table")
}

#[test]
fn the_retail_height_table_passes_the_clients_own_validation() {
    let s = store();
    let bytes = s.read_typed(DbType::Region, REGION_ID).expect("region");
    let region = Region::decode_payload(REGION_ID, &bytes).expect("region decodes");
    let ld = &region.land_defs;

    //  hard-codes these; the Region file carries the same values.
    assert_eq!(ld.num_block_length, 0xFF);
    assert_eq!(ld.num_block_width, 0xFF);
    assert_eq!(ld.square_length, 24.0);
    assert_eq!(ld.lblock_length, 8);
    assert_eq!(ld.vertex_per_cell, 1);
    assert_eq!(ld.max_obj_height, 200.0);
    assert_eq!(ld.sky_height, 1000.0);
    assert_eq!(ld.road_width, 5.0);

    let table = landdefs::validate_height_table(&ld.land_height_table).expect("accepted");
    assert_eq!(table.len(), 256);
    assert_eq!(table[0], 0.0);
    // ACE's earlier description, "twice the stored height byte", holds
    // only up to index 201. Prove both halves of that here, because getting it wrong misplaces
    // every high vertex in the game.
    #[allow(clippy::cast_precision_loss)]
    let first_nonlinear = (0..256)
        .find(|&i| table[i] != i as f32 * 2.0)
        .expect("a break exists");
    assert_eq!(
        first_nonlinear, 201,
        "the doubling rule holds for the first 201 entries (0..=200) and breaks at 201"
    );
    for (i, v) in table.iter().enumerate().take(first_nonlinear) {
        #[allow(clippy::cast_precision_loss)]
        let doubled = i as f32 * 2.0;
        assert_eq!(*v, doubled, "entry {i} should still be 2*i");
    }
    assert_eq!(
        table[201], 404.0,
        "the first non-linear entry, which ACE's comment misplaces"
    );
    assert!(
        table[255] > 400.0 && table[255] <= 800.0,
        "top entry {}",
        table[255]
    );
    assert!(
        table.windows(2).all(|w| w[1] >= w[0]),
        "the table must be monotonic"
    );
}

/// The split flag, computed the way ACE's `LandblockMesh.cs:167` writes it. Deliberately a
/// separate expression from `dereth_physics::land::split_hash`, which follows the client's nesting.
fn ace_landblock_mesh_split(x: u32, y: u32) -> bool {
    let dw = x
        .wrapping_mul(y)
        .wrapping_mul(0x0CCA_C033)
        .wrapping_sub(x.wrapping_mul(0x421B_E3BD))
        .wrapping_add(y.wrapping_mul(0x6C1A_C587))
        .wrapping_sub(0x519B_8F25);
    // ACE converts to an unsigned float, scales by 2^-32 and compares against 0.5.
    (f64::from(dw) * 2.0_f64.powi(-32)) >= 0.5
}

#[test]
fn every_retail_landblock_builds_and_its_geometry_matches_the_dat() {
    let s = store();
    let table = height_table(&s);

    let ids = s.ids_of(DbType::LandBlock);
    assert!(!ids.is_empty(), "landblocks are exercised");
    let input_count = ids.len() as u64;

    let mut blocks = 0_u32;
    let mut vertices_checked = 0_u64;
    let mut splits_checked = 0_u64;
    let mut water_cells = 0_u64;
    let mut road_vertices = 0_u64;
    let mut with_info = 0_u32;

    for id in &ids {
        let bytes = s
            .read_typed(DbType::LandBlock, *id)
            .expect("landblock read");
        let lb = CellLandblock::decode_payload(*id, &bytes).expect("landblock decodes");
        #[allow(clippy::cast_possible_truncation)]
        let lbid = LandblockId((id.0 >> 16) as u16);

        let block = LandblockCollision::build(
            lbid,
            Box::new(lb.height),
            Box::new(lb.terrain),
            lb.lbi_exists != 0,
            8,
            &table,
        )
        .expect("every retail landblock is full detail");

        if block.has_info {
            with_info += 1;
        }

        // The vertex construction: x = i * 24, y = j * 24, z = table[height[i * 9 + j]].
        for i in 0..SIDE_VERTEX_COUNT {
            for j in 0..SIDE_VERTEX_COUNT {
                let v = block.vertices[i * SIDE_VERTEX_COUNT + j];
                #[allow(clippy::cast_precision_loss)]
                {
                    assert_eq!(v.x, i as f32 * 24.0);
                    assert_eq!(v.y, j as f32 * 24.0);
                }
                assert_eq!(v.z, table[block.height[i * SIDE_VERTEX_COUNT + j] as usize]);
                vertices_checked += 1;
            }
        }

        // SWtoNEcut against ACE's mesh expression, over global cell coordinates.
        let (lbx8, lby8) = (u32::from(lbid.x()) * 8, u32::from(lbid.y()) * 8);
        for i in 0..8_u32 {
            for j in 0..8_u32 {
                let expect = ace_landblock_mesh_split(lbx8 + i, lby8 + j);
                assert_eq!(
                    block.split[(i * 8 + j) as usize],
                    expect,
                    "split flag for global cell ({}, {})",
                    lbx8 + i,
                    lby8 + j
                );
                splits_checked += 1;
            }
        }

        // Per-cell water from the four corner vertices, recomputed here from the raw words.
        for i in 0..8_usize {
            for j in 0..8_usize {
                let (any, all) = calc_cell_water(&block.terrain, i, j);
                let expect = if !any {
                    WaterType::NotWater
                } else if all {
                    WaterType::EntirelyWater
                } else {
                    WaterType::PartiallyWater
                };
                assert_eq!(block.cell_water[i * 8 + j], expect);
                if expect != WaterType::NotWater {
                    water_cells += 1;
                }
            }
        }

        road_vertices += block.terrain.iter().filter(|w| *w & 3 != 0).count() as u64;
        blocks += 1;
    }

    assert_eq!(blocks as u64, input_count);
    assert_eq!(vertices_checked, input_count * 81);
    assert_eq!(splits_checked, input_count * 64);
    // Sanity: the world has water and roads, so a rule that silently produced none would be
    // caught here rather than by a trace much later.
    assert!(
        water_cells > 0,
        "only {water_cells} water cells across the world"
    );
    assert!(
        road_vertices > 0,
        "only {road_vertices} road vertices across the world"
    );
    assert!(
        with_info > 0,
        "only {with_info} blocks carry a LandblockInfo record"
    );
    eprintln!(
        "Landscape: {blocks} landblocks, {vertices_checked} vertices, {splits_checked} split flags, \
         {water_cells} water cells, {road_vertices} road vertices, {with_info} with info"
    );
}

/// Landblock edges agree with their neighbours across the whole world.
#[test]
fn landblock_edges_agree_with_their_neighbours_across_the_whole_world() {
    let s = store();

    let read = |x: u8, y: u8| -> Option<CellLandblock> {
        let id = DataId((u32::from(x) << 24) | (u32::from(y) << 16) | 0xFFFF);
        let bytes = s.read_typed(DbType::LandBlock, id).ok()?;
        CellLandblock::decode_payload(id, &bytes).ok()
    };

    let mut east_seams = 0_u64;
    let mut north_seams = 0_u64;
    for x in 0..0xFF_u8 {
        let mut row: Vec<Option<CellLandblock>> = Vec::with_capacity(256);
        for y in 0..0xFF_u8 {
            row.push(read(x, y));
        }
        let east: Vec<Option<CellLandblock>> = if x + 1 < 0xFF {
            (0..0xFF_u8).map(|y| read(x + 1, y)).collect()
        } else {
            Vec::new()
        };
        for y in 0..0xFF_usize {
            let Some(here) = row[y].as_ref() else {
                continue;
            };
            // North neighbour: this block's j = 8 column is the neighbour's j = 0 column.
            if y + 1 < 0xFF {
                if let Some(n) = row[y + 1].as_ref() {
                    for i in 0..SIDE_VERTEX_COUNT {
                        assert_eq!(
                            here.height[i * SIDE_VERTEX_COUNT + 8],
                            n.height[i * SIDE_VERTEX_COUNT],
                            "north seam of block ({x:#04X}, {y:#04X}) at i = {i}"
                        );
                        north_seams += 1;
                    }
                }
            }
            // East neighbour: this block's i = 8 row is the neighbour's i = 0 row.
            if let Some(e) = east.get(y).and_then(|o| o.as_ref()) {
                for j in 0..SIDE_VERTEX_COUNT {
                    assert_eq!(
                        here.height[8 * SIDE_VERTEX_COUNT + j],
                        e.height[j],
                        "east seam of block ({x:#04X}, {y:#04X}) at j = {j}"
                    );
                    east_seams += 1;
                }
            }
        }
    }
    assert!(
        east_seams > 0 && north_seams > 0,
        "{east_seams} / {north_seams}"
    );
    eprintln!("Landscape: {east_seams} east-seam and {north_seams} north-seam vertices agree");
}

/// Block-edge continuity: because the split hash runs over global
/// cell coordinates, the diagonal a cell gets is the same whichever block computed it. Checked by
/// building the two blocks either side of a seam and comparing the cells that share an edge.
#[test]
fn the_split_hash_is_seam_independent_on_retail_blocks() {
    let s = store();
    let table = height_table(&s);
    let build = |x: u8, y: u8| {
        let id = DataId((u32::from(x) << 24) | (u32::from(y) << 16) | 0xFFFF);
        let bytes = s.read_typed(DbType::LandBlock, id).expect("read");
        let lb = CellLandblock::decode_payload(id, &bytes).expect("decode");
        LandblockCollision::build(
            LandblockId::new(x, y),
            Box::new(lb.height),
            Box::new(lb.terrain),
            lb.lbi_exists != 0,
            8,
            &table,
        )
        .expect("full detail")
    };
    let mut compared = 0_u32;
    for (x, y) in [(0xA9_u8, 0xB4_u8), (0x00, 0x00), (0x7F, 0x7F), (0xFD, 0xFD)] {
        let here = build(x, y);
        let east = build(x + 1, y);
        // Cell (7, j) of `here` and cell (0, j) of `east` are adjacent, not identical, so what is
        // compared is that each block's flag equals the global-coordinate hash - i.e. neither
        // block used its own local index.
        for j in 0..8_u32 {
            assert_eq!(
                here.split[(7 * 8 + j) as usize],
                split_hash(u32::from(x) * 8 + 7, u32::from(y) * 8 + j)
            );
            assert_eq!(
                east.split[j as usize],
                split_hash(u32::from(x + 1) * 8, u32::from(y) * 8 + j)
            );
            compared += 2;
        }
    }
    assert_eq!(compared, 64);
}

/// The terrain word's water bits, and the deep-sea collision stop: count how
/// many landblocks are entirely water, because that is the population the land cell's
/// environment-collision search refuses entry to.
#[test]
fn the_world_contains_entirely_water_landblocks() {
    let s = store();
    let table = height_table(&s);
    let ids = s.ids_of(DbType::LandBlock);
    let mut entirely = 0_u32;
    let mut partially = 0_u32;
    let mut dry = 0_u32;
    for id in &ids {
        let bytes = s.read_typed(DbType::LandBlock, *id).expect("read");
        let lb = CellLandblock::decode_payload(*id, &bytes).expect("decode");
        #[allow(clippy::cast_possible_truncation)]
        let lbid = LandblockId((id.0 >> 16) as u16);
        let b = LandblockCollision::build(
            lbid,
            Box::new(lb.height),
            Box::new(lb.terrain),
            lb.lbi_exists != 0,
            8,
            &table,
        )
        .expect("full detail");
        match b.water_type {
            WaterType::EntirelyWater => entirely += 1,
            WaterType::PartiallyWater => partially += 1,
            WaterType::NotWater => dry += 1,
        }
    }
    assert_eq!((entirely + partially + dry) as usize, ids.len());
    assert!(
        entirely > 0,
        "only {entirely} deep-sea blocks; 5.10 would be untestable"
    );
    assert!(partially > 0, "only {partially} mixed blocks");
    eprintln!(
        "Landscape: {entirely} entirely-water, {partially} partially-water, {dry} dry landblocks"
    );
}

/// Every terrain type present in the retail data, and the fact that the five water types are the
/// only ones `TERRAIN_SURF_CHAR` calls water.
#[test]
fn only_terrain_types_16_to_20_appear_as_water_in_retail_data() {
    let s = store();
    let ids = s.ids_of(DbType::LandBlock);
    let mut seen = [0_u64; 32];
    for id in ids.iter().step_by(7) {
        let bytes = s.read_typed(DbType::LandBlock, *id).expect("read");
        let lb = CellLandblock::decode_payload(*id, &bytes).expect("decode");
        for w in lb.terrain {
            seen[((w >> 2) & 0x1F) as usize] += 1;
        }
    }
    let present: Vec<usize> = (0..32).filter(|&i| seen[i] > 0).collect();
    assert!(
        !present.is_empty(),
        "only {} terrain types in use",
        present.len()
    );
    for &i in &present {
        #[allow(clippy::cast_possible_truncation)]
        let word = (i as u16) << 2;
        assert_eq!(
            surf_char(word) == SurfChar::Water,
            (16..=20).contains(&i),
            "terrain type {i} water classification"
        );
    }
    eprintln!("Landscape: terrain types present in retail data: {present:?}");
}

/// The seam test discriminates x major from y major.
#[test]
fn the_seam_test_discriminates_x_major_from_y_major() {
    let s = store();
    let read = |x: u8, y: u8| -> Option<CellLandblock> {
        let id = DataId((u32::from(x) << 24) | (u32::from(y) << 16) | 0xFFFF);
        let bytes = s.read_typed(DbType::LandBlock, id).ok()?;
        CellLandblock::decode_payload(id, &bytes).ok()
    };
    let mut checked = 0_u32;
    let mut transposed_would_fail = 0_u32;
    for x in (0..0xFE_u8).step_by(7) {
        for y in (0..0xFF_u8).step_by(7) {
            let (Some(here), Some(east)) = (read(x, y), read(x + 1, y)) else {
                continue;
            };
            checked += 1;
            // x-major (the client's rule) must always hold; that is the other test.
            // y-major would compare here(j, 8) against east(j, 0).
            let differs = (0..SIDE_VERTEX_COUNT).any(|j| {
                here.height[j * SIDE_VERTEX_COUNT + 8] != east.height[j * SIDE_VERTEX_COUNT]
            });
            if differs {
                transposed_would_fail += 1;
            }
        }
    }
    assert!(checked > 0, "only {checked} block pairs sampled");
    let ratio = f64::from(transposed_would_fail) / f64::from(checked);
    assert!(
        ratio > 0.5,
        "only {transposed_would_fail} of {checked} sampled seams would reject a y-major reader"
    );
    eprintln!("Landscape: {transposed_would_fail}/{checked} seams reject a y-major reading");
}
