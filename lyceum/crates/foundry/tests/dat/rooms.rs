//! Behaviour: none (tooling: which indoor cell a point is in, over the retail files)

use dereth_primitives::frame::V3;
use dereth_primitives::Vec3;
use lyceum_foundry::edit::Session;

const HOLTBURG: u16 = 0xA9B4;

#[test]
fn every_holtburg_room_holds_its_own_middle_and_no_room_holds_the_street() {
    let (_dir, world) = super::sample::sample("pack-rooms");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let shapes = s.cell_shapes(HOLTBURG);
    let cells = s.cells(HOLTBURG);
    assert_eq!(shapes.len(), cells.len());
    for (index, cell) in &cells {
        let env = s.environment(cell.environment).unwrap();
        let cs = &env.cells[usize::from(cell.cell_struct)];
        #[allow(clippy::cast_precision_loss)]
        let n = cs.vertex_array.vertices.len() as f32;
        let sum = cs.vertex_array.vertices.iter().fold(Vec3::ZERO, |a, v| {
            a.add(dereth_primitives::frame::localtoglobal(
                &cell.frame,
                v.position,
            ))
        });
        let middle = sum.mul(1.0 / n);
        let holding: Vec<u16> = shapes
            .iter()
            .filter(|r| r.holds(middle))
            .map(|r| r.index)
            .collect();
        assert!(holding.contains(index), "0x{index:04X}: {holding:04X?}");
        // Holtburg's rooms are a town's: every one is seen from outside.
        assert!(shapes.iter().any(|r| r.index == *index && r.seen_outside));
    }
    // The middle of the landblock's south edge, at the ground: the street, in no room.
    let street = Vec3::new(96.0, 1.0, s.ground(HOLTBURG, 96.0, 1.0).unwrap() + 1.0);
    assert!(shapes.iter().all(|r| !r.holds(street)));
}

#[test]
fn the_worlds_dungeons_are_the_landblocks_whose_rooms_are_not_seen_from_outside() {
    let (_dir, world) = super::sample::sample("pack-dungeons");
    let s = Session::open(&world, &dereth_dat::testing::dat_dir(), None).unwrap();
    let started = std::time::Instant::now();
    let retail = s.retail_dungeons();
    println!("{} dungeons in {:?}", retail.len(), started.elapsed());
    // The Drudge Hideout is one; Holtburg's houses are a town's.
    assert!(retail.contains_key(&0x019E));
    assert!(!retail.contains_key(&HOLTBURG));
    assert!(retail.len() > 1_000, "{}", retail.len());
    // With no server read, none is named.
    let all = s.dungeons(&retail);
    assert_eq!(all.len(), retail.len());
    assert!(all.iter().all(|d| d.name.is_none() && !d.pack));
}
