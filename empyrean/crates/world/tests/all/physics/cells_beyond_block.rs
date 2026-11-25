//! ACE: Source/ACE.Server/Physics/Common/EnvCell.cs::find_transit_cells
//! A world's dungeon whose rooms stand beyond their landblock's 192 m, as a converted land's
//! dungeon under its entrance does, is walked as one inside it: a cell's place is landblock
//! meters, any distance from the landblock's corner. A body walking from one such room into the
//! next changes cell as it does with the rooms where retail has them, a wall stops it at the same
//! place, and a portal's move takes it out onto the land.
//! Fixture: the retail dats in the real-content tier, with an overlay written by the test that
//! moves a retail dungeon's every cell 200 m east.

#[cfg(feature = "real-content")]
mod real_content {
    use std::sync::Arc;
    use std::time::Duration;

    use dereth_assets::world::{EnvCell, LandblockInfo};
    use dereth_assets::Decode;
    use dereth_dat::overlay::{container_name, OverlayWriter};
    use dereth_dat::{DatFile, ModernDat};
    use dereth_primitives::frame::V3;
    use dereth_primitives::{CellId, DataId, Frame, Position, Quat, Vec3};
    use empyrean_common::clock::ClockSnapshot;
    use empyrean_common::dotnet::datetime::DotNetDateTime;
    use empyrean_dat::{DatManager, RealDats};
    use empyrean_world::physics::phys_ext;
    use empyrean_world::World;

    /// Holtburg's dungeon, its cells joined by portals.
    const DUNGEON: u16 = 0x01F6;
    /// How far east the overlay moves its cells: past the landblock's 192 m.
    const EAST: f32 = 200.0;

    fn world(dats: Arc<DatManager>) -> World {
        let mut w = World::new(
            ClockSnapshot {
                portal_year_ticks: 0.0,
                unix_time: 0.0,
                utc: DotNetDateTime::new(2026, 1, 1),
                monotonic: Duration::ZERO,
            },
            dats,
        );
        phys_ext::load_landblock(&mut w, DUNGEON);
        w
    }

    /// The dungeon's cells as the retail files hold them.
    fn cells(file: &DatFile) -> Vec<EnvCell> {
        let lid = (u32::from(DUNGEON) << 16) | 0xFFFE;
        let info =
            LandblockInfo::decode_payload(DataId(lid), &file.read(DataId(lid)).unwrap()).unwrap();
        (0..info.num_cells)
            .map(|k| {
                let id = DataId((u32::from(DUNGEON) << 16) | (0x0100 + k));
                EnvCell::decode_payload(id, &file.read(id).unwrap()).unwrap()
            })
            .collect()
    }

    /// The retail files with an overlay in `dir` that moves every cell of the dungeon `EAST`
    /// meters east, nothing else changed.
    fn moved(dir: &std::path::Path, file: &DatFile) -> Arc<DatManager> {
        let mut w = OverlayWriter::open_or_create(
            &dir.join(container_name(ModernDat::Cell)),
            file,
            "client_cell_1.dat",
            "cells-beyond",
            0,
        )
        .unwrap();
        let it = dereth_dat::decompose::revision_over(file) + 1;
        for mut c in cells(file) {
            c.frame.origin.x += EAST;
            let version = file
                .base_entry(c.id)
                .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1));
            w.put(c.id, &c.encode(), version, it, 0).unwrap();
        }
        w.add_iteration(it, 0).unwrap();
        w.flush(0).unwrap();
        drop(w);
        let real = RealDats::open(&dereth_dat::testing::dat_dir())
            .unwrap()
            .with_overlay(dir)
            .unwrap();
        DatManager::initialize(Arc::new(real)).unwrap()
    }

    /// A drudge's body placed in cell `cell` at `from` walks toward `to` in 0.8 m steps, each a
    /// committed transition, until it arrives or cannot move: where it ends, cell and all.
    fn walk(w: &mut World, cell: u32, from: Vec3, to: Vec3) -> Position {
        let h = phys_ext::make_object(w, 0x0200_07DD, 0x7100_0001, true);
        let start = Position::new(CellId(cell), Frame::new(from, Quat::IDENTITY));
        assert!(phys_ext::enter_world(w, h, &start), "the drudge is placed");
        for _ in 0..60 {
            let cur = phys_ext::position(w, h).expect("a body");
            let d = to.sub(cur.frame.origin);
            let len = (d.x * d.x + d.y * d.y).sqrt();
            if len < 0.01 {
                break;
            }
            let step = len.min(0.8);
            let mut next = cur;
            next.frame.origin.x += d.x / len * step;
            next.frame.origin.y += d.y / len * step;
            let Some(t) = w.physics.transition(h, &cur, &next, false) else {
                break;
            };
            phys_ext::set_position_internal(w, h, &t);
        }
        let end = phys_ext::position(w, h).expect("a body");
        phys_ext::destroy_object(w, h);
        end
    }

    fn apart(a: Vec3, b: Vec3) -> f32 {
        let d = a.sub(b);
        (d.x * d.x + d.y * d.y + d.z * d.z).sqrt()
    }

    fn east(v: Vec3) -> Vec3 {
        Vec3::new(v.x + EAST, v.y, v.z)
    }

    /// ACE: Source/ACE.Server/Physics/Common/EnvCell.cs::find_transit_cells
    #[test]
    fn rooms_beyond_their_landblock_are_walked_through_and_left_as_rooms_where_retail_has_them() {
        let file = DatFile::open(&ModernDat::Cell.in_dir(&dereth_dat::testing::dat_dir()))
            .expect("the retail cell file");
        let scratch = dereth_dat::testing::ScratchDir::new("cells-beyond").expect("scratch");
        let mut retail = world(
            empyrean_dat::DatManager::initialize(Arc::new(
                RealDats::open(&dereth_dat::testing::dat_dir()).unwrap(),
            ))
            .unwrap(),
        );
        let mut beyond = world(moved(scratch.path(), &file));
        let all = cells(&file);
        assert!(
            all.iter().all(|c| c.frame.origin.x + EAST > 192.0),
            "every cell moved beyond the landblock"
        );

        // A walk from a room into the one its portal joins: the first pair the retail walk gets
        // through ends in the same cell, at the same place EAST meters on.
        let mut walked = 0;
        for c in &all {
            for p in c.portals.iter().filter(|p| p.other_cell_id >> 16 == 0) {
                let Some(d) = all.iter().find(|d| d.id.0 & 0xFFFF == p.other_cell_id) else {
                    continue;
                };
                let (from, to) = (c.frame.origin, d.frame.origin);
                if apart(from, to) < 2.0 {
                    continue;
                }
                let there = walk(&mut retail, c.id.0, from, to);
                if there.cell.0 != d.id.0 {
                    continue;
                }
                let moved = walk(&mut beyond, c.id.0, east(from), east(to));
                assert_eq!(
                    moved.cell.0, d.id.0,
                    "into {:08X} beyond the landblock",
                    d.id.0
                );
                assert!(
                    apart(moved.frame.origin, east(there.frame.origin)) < 0.01,
                    "where the retail walk ends, {EAST} m on: {moved:?} {there:?}"
                );
                walked += 1;
                break;
            }
            if walked == 3 {
                break;
            }
        }
        assert_eq!(walked, 3, "three walks between rooms");

        // A wall stops it at the same place: 40 m straight on from the first room, through no
        // portal the walk can find.
        let c = &all[0];
        let far = c.frame.origin.add(Vec3::new(0.0, 40.0, 0.0));
        let stopped = walk(&mut retail, c.id.0, c.frame.origin, far);
        let moved = walk(&mut beyond, c.id.0, east(c.frame.origin), east(far));
        assert!(
            apart(stopped.frame.origin, far) > 1.0,
            "the retail walk is stopped short: {stopped:?}"
        );
        assert_eq!(moved.cell, stopped.cell, "stopped in the same cell");
        assert!(
            apart(moved.frame.origin, east(stopped.frame.origin)) < 0.01,
            "stopped at the same place, {EAST} m on: {moved:?} {stopped:?}"
        );

        // A portal's move takes it from a room beyond the landblock out onto the land.
        phys_ext::load_landblock(&mut beyond, 0xA9B4);
        let h = phys_ext::make_object(&mut beyond, 0x0200_07DD, 0x7100_0002, true);
        let inside = Position::new(
            CellId(c.id.0),
            Frame::new(east(c.frame.origin), Quat::IDENTITY),
        );
        assert!(phys_ext::enter_world(&mut beyond, h, &inside));
        // Holtburg's land cell 0x1E: x 72 to 96, y 120 to 144.
        let land = Position::new(
            CellId(0xA9B4_001E),
            Frame::new(Vec3::new(84.0, 141.0, 42.0), Quat::IDENTITY),
        );
        assert!(phys_ext::set_position(&mut beyond, h, &land), "moved out");
        let out = phys_ext::position(&beyond, h).expect("a body");
        assert_eq!(out.cell, land.cell, "on the land, out of the room");
        let (o, l) = (out.frame.origin, land.frame.origin);
        assert!(
            (o.x - l.x).abs() < 0.01 && (o.y - l.y).abs() < 0.01,
            "where the portal put it, on the ground: {out:?}"
        );
        phys_ext::destroy_object(&mut beyond, h);
    }
}
