//! Behaviour: none (how this client keeps the cell traversal's record between frames, checked against a record built afresh)

use super::*;
use dereth_world_render::cells::portal_view::{CellPortal, CellSource};
use std::sync::Arc;

fn gpu() -> Gpu {
    Gpu::new(
        None,
        &dereth_render::device::DeviceConfig {
            width: 16,
            height: 16,
            ..Default::default()
        },
    )
    .expect("required headless WARP device")
}

/// Every resident cell's portals as the traversal reads them, in cell order.
fn facts(cells: &ViewCells<'_>) -> Vec<(u32, Vec<CellPortal>)> {
    cells
        .base
        .slot
        .keys()
        .map(|id| (*id, cells.cell(*id).expect("resident").portals.clone()))
        .collect()
}

/// How many cells' viewpoint facts have been worked out.
fn worked_out(cells: &ViewCells<'_>) -> usize {
    cells
        .facts
        .cells
        .iter()
        .filter(|c| c.get().is_some())
        .count()
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn the_kept_cell_record_matches_one_built_afresh_after_the_camera_moves() {
    let store = dereth_dat::testing::open_store().expect("required installed retail DATs");
    let mut gpu = gpu();
    let mut scene = WorldScene::load(
        &store,
        &mut gpu,
        SceneConfig {
            landblock: 0xA9B4,
            land_radius: 0,
            scenery_radius: 0,
            character: false,
            particles: false,
            ..Default::default()
        },
    )
    .expect("Holtburg's block loads");

    let first = scene.draw.traversal_cells(&scene.world).0;
    assert!(first.cell_count() > 1, "Holtburg's houses have rooms");
    assert_eq!(
        worked_out(&first),
        0,
        "nothing is worked out before it is asked for"
    );
    let some = *first.base.slot.keys().next().expect("a cell");
    assert!(first.cell(some).is_some());
    assert_eq!(
        worked_out(&first),
        1,
        "asking for a cell works out that cell alone"
    );
    let again = scene.draw.traversal_cells(&scene.world).0;
    assert!(
        Arc::ptr_eq(&first.facts, &again.facts),
        "a second call from the same place shares the first one's facts"
    );
    drop(again);

    let p = scene.world.camera.position;
    scene.world.camera.position = Vec3::new(p.x + 3.0, p.y - 2.0, p.z + 1.5);
    let moved = scene.draw.traversal_cells(&scene.world).0;
    assert!(
        Arc::ptr_eq(&first.base, &moved.base),
        "a camera move keeps which cells are resident"
    );
    assert_ne!(
        facts(&first),
        facts(&moved),
        "the viewpoint facts follow the camera"
    );

    *scene.draw.cell_records.borrow_mut() = CellRecords::default();
    let afresh = scene.draw.traversal_cells(&scene.world).0;
    assert!(!Arc::ptr_eq(&moved.base, &afresh.base));
    assert_eq!(
        facts(&moved),
        facts(&afresh),
        "the kept record says what one built afresh from the same place says"
    );
}
