use super::*;

// A building's openings are read per degrade level of its shell, through the production
// building bake, over Holtburg's landblock 0xA9B4 in the retail dats.

/// The cottage `0x01000830`, whose degrade record's level 1 is `0x01000907`.
const COTTAGE: DataId = DataId(0x0100_0830);
const COTTAGE_LEVEL_1: DataId = DataId(0x0100_0907);

/// Behaviour: rendering.interior.a-shells-openings-follow-its-detail-level
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn holtburgs_building_shells_have_openings_at_their_full_detail_level_only() {
    let store = dereth_dat::testing::open_store().expect("required installed retail DATs");
    let (bx, by) = dereth_world_data::landblock::block_xy(0xA9B4);
    let info =
        dereth_client_runtime::world_build::read_lbi(&store, bx, by).expect("block 0xA9B4's info");
    let with_portals = info
        .buildings
        .iter()
        .filter(|b| !b.portals.is_empty())
        .count();
    assert!(
        with_portals >= 7,
        "Holtburg has {with_portals} buildings with doors"
    );
    let none = vec![None; info.buildings.len()];

    let mut cache = BakeCache::default();
    let views = bake_building_views((&store, &mut cache), None, 0xA9B4, &info, &[], &none, true);
    assert_eq!(
        views.len(),
        with_portals,
        "every building with doors has a view"
    );
    let mut degrading = 0;
    for (b, v) in info
        .buildings
        .iter()
        .filter(|b| !b.portals.is_empty())
        .zip(&views)
    {
        assert_eq!(b.frame, v.frame, "views are in the buildings' order");
        let record = cache.degrade_record(&store, b.id);
        eprintln!(
            "{:#010X}: {} level(s), openings {:?}",
            b.id.0,
            v.levels.len(),
            v.levels
                .iter()
                .map(|l| l.as_ref().map(|o| o.portal_polygons.len()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            v.levels.len(),
            record.as_ref().map_or(1, |r| r.degrades.len()),
            "{:#010X}: one entry per level of the shell's record",
            b.id.0
        );
        assert!(
            v.levels[0].is_some(),
            "{:#010X}: the full-detail shell has its doors",
            b.id.0
        );
        for (level, openings) in v.levels.iter().enumerate().skip(1) {
            assert!(
                openings.is_none(),
                "{:#010X}: level {level} has openings, but no degraded Holtburg shell does",
                b.id.0
            );
        }
        if v.levels.len() > 1 {
            degrading += 1;
        }
    }
    assert!(
        degrading >= 7,
        "only {degrading} of Holtburg's buildings with doors degrade"
    );

    // The cottage, read level by level: its full-detail model opens, its level-1 model does not.
    let cottage = cache
        .degrade_record(&store, COTTAGE)
        .expect("the cottage's degrade record");
    assert_eq!(cottage.degrades[1].gfxobj_id, COTTAGE_LEVEL_1);
    assert!(level_openings(&store, COTTAGE).is_some());
    assert!(level_openings(&store, COTTAGE_LEVEL_1).is_none());
    assert!(level_openings(&store, DataId(0)).is_none());

    // With the degrade switches off the bake keeps the one full-detail entry and no placement,
    // as the shell bake does.
    let flat = bake_building_views((&store, &mut cache), None, 0xA9B4, &info, &[], &none, false);
    assert_eq!(flat.len(), with_portals);
    assert!(flat
        .iter()
        .all(|v| v.levels.len() == 1 && v.levels[0].is_some() && v.shell_placement.is_none()));
}

/// Behaviour: rendering.interior.a-shells-openings-follow-its-detail-level
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_building_follows_the_level_its_shell_placement_draws() {
    let store = dereth_dat::testing::open_store().expect("required installed retail DATs");
    let (bx, by) = dereth_world_data::landblock::block_xy(0xA9B4);
    let info =
        dereth_client_runtime::world_build::read_lbi(&store, bx, by).expect("block 0xA9B4's info");
    let k = info
        .buildings
        .iter()
        .position(|b| b.id == COTTAGE)
        .expect("the cottage is in Holtburg");
    // The production shell bake numbers the cottage's shell part.
    let mut cache = BakeCache::default();
    let mut baker = ObjectBaker::new(&store, &mut cache, true, true, false);
    let shell = baker.add_object_kind(COTTAGE, &info.buildings[k].frame, 1.0, true);
    assert_eq!(
        shell,
        Some(0),
        "the shell part is the cottage's first placement"
    );
    let mut degrade = std::mem::take(&mut baker.placements);
    drop(baker);
    let mut shells = vec![None; info.buildings.len()];
    shells[k] = shell;
    let views = bake_building_views(
        (&store, &mut cache),
        None,
        0xA9B4,
        &info,
        &[],
        &shells,
        true,
    );
    let v = views
        .iter()
        .find(|v| v.frame == info.buildings[k].frame)
        .expect("the cottage's view");
    assert_eq!(v.shell_placement, Some(0));
    assert_eq!(v.drawn_level(&degrade), 0);
    assert!(v.drawn_openings(&degrade).is_some(), "level 0 opens");
    degrade[0].level = 1;
    assert_eq!(v.drawn_level(&degrade), 1);
    assert!(
        v.drawn_openings(&degrade).is_none(),
        "the level-1 cottage opens nothing"
    );
    assert!(
        v.full_openings().is_some(),
        "the doors are still where they are"
    );
}

/// Behaviour: rendering.interior.a-shells-openings-follow-its-detail-level
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_shell_another_eras_look_draws_opens_through_that_looks_degrade_record() {
    use dereth_assets::motion::{GfxObjDegradeInfo, GfxObjInfo};
    let store = dereth_dat::testing::open_store().expect("required installed retail DATs");
    let (bx, by) = dereth_world_data::landblock::block_xy(0xA9B4);
    let info =
        dereth_client_runtime::world_build::read_lbi(&store, bx, by).expect("block 0xA9B4's info");
    let k = info
        .buildings
        .iter()
        .position(|b| b.id == COTTAGE)
        .expect("the cottage is in Holtburg");
    let none = vec![None; info.buildings.len()];
    // The look's files give the cottage a record whose level 1 keeps the full-detail model, so
    // the shell they draw keeps its doors one level further out than the world's does.
    let level = |id: DataId| GfxObjInfo {
        gfxobj_id: id,
        degrade_mode: 1,
        min_dist: 0.0,
        ideal_dist: 0.0,
        max_dist: 0.0,
    };
    let mut look_cache = BakeCache::default();
    look_cache.degrades.insert(
        COTTAGE,
        Some(Arc::new(GfxObjDegradeInfo {
            id: DataId(0x1100_0000),
            degrades: vec![level(COTTAGE), level(COTTAGE)],
        })),
    );
    let mut from_look = vec![false; info.buildings.len()];
    from_look[k] = true;
    let opens = |views: &[BuildingView]| -> Vec<bool> {
        views
            .iter()
            .find(|v| v.frame == info.buildings[k].frame)
            .expect("the cottage's view")
            .levels
            .iter()
            .map(Option::is_some)
            .collect()
    };

    let mut cache = BakeCache::default();
    let world = bake_building_views(
        (&store, &mut cache),
        Some((&store, &mut look_cache)),
        0xA9B4,
        &info,
        &vec![false; info.buildings.len()],
        &none,
        true,
    );
    let world = opens(&world);
    assert!(
        world.len() > 2 && world[0] && world[1..].iter().all(|o| !o),
        "the world's cottage opens at full detail only, at {world:?}"
    );
    let drawn_by_look = bake_building_views(
        (&store, &mut cache),
        Some((&store, &mut look_cache)),
        0xA9B4,
        &info,
        &from_look,
        &none,
        true,
    );
    assert_eq!(
        opens(&drawn_by_look),
        [true, true],
        "the look's cottage opens at both of its record's levels"
    );
}
