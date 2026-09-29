//! Vectors: fixtures/vectors/position_real/
//! Tests of positions.
//! Fixture: isolated world state and the shared area fixtures.

mod coordinates {
    use crate::support::position_and_inventory::*;

    /// `ACEPosition` / `PhysPosition`: the cell, origin and rotation carried across unchanged.
    #[test]
    fn positions_convert_to_and_from_the_physics_engine() {
        let p = Position::from_vectors(
            0xA9B4_0019,
            Vector3::new(84.0, 7.1, 94.005),
            Quaternion::new(
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ),
        );
        let phys = position_extensions::phys_position(&p);
        assert_eq!(phys.cell.0, 0xA9B4_0019);
        assert_eq!(
            (
                phys.frame.origin.x,
                phys.frame.origin.y,
                phys.frame.origin.z
            ),
            (84.0, 7.1, 94.005)
        );
        assert_eq!(
            (phys.frame.rotation.w, phys.frame.rotation.z),
            (
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2
            )
        );
        let back = position_extensions::ace_position(&phys);
        assert_eq!(
            (
                back.cell(),
                back.position_x,
                back.position_y,
                back.position_z
            ),
            (p.cell(), p.position_x, p.position_y, p.position_z)
        );
        assert_eq!(
            (
                back.rotation_w,
                back.rotation_x,
                back.rotation_y,
                back.rotation_z
            ),
            (p.rotation_w, p.rotation_x, p.rotation_y, p.rotation_z)
        );
    }

    /// `AdjustMapCoords` on the synthetic flat land: Z to the terrain (0 here), no building; a
    /// position in landblock row or column 0xFF is ACE's IndexOutOfRangeException
    /// (`LandblockManager.GetLandblock`) and is left alone.
    #[test]
    fn adjust_map_coords_puts_the_position_on_the_ground() {
        let mut w = world();
        let mut p = Position::from_components(
            u32::from(LB) << 16 | 0x0019,
            84.0,
            7.1,
            55.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        );
        position_extensions::adjust_map_coords(&mut w, &mut p).expect("a landblock");
        assert_eq!(
            (p.cell(), p.position_z),
            (u32::from(LB) << 16 | 0x0019, 0.0)
        );

        let mut edge =
            Position::from_components(0xFF10_0001, 1.0, 1.0, 5.0, 0.0, 0.0, 0.0, 1.0, false);
        assert!(position_extensions::adjust_map_coords(&mut w, &mut edge)
            .unwrap_err()
            .contains("IndexOutOfRangeException"));
        assert_eq!(edge.position_z, 5.0);
    }
}

#[cfg(feature = "real-content")]
mod coordinates_real {
    mod real_content {
        use crate::support::position_and_inventory::real_content::*;

        #[test]
        fn adjust_map_coords_matches_ace_on_the_retail_dats() {
            let mut w = real_world();
            let mut checked = 0;
            for case in &vectors::load_named("position_real", "adjust_map_coords").cases {
                let (ns, ew) = (
                    f32_of(&case.input["ns"]).unwrap(),
                    f32_of(&case.input["ew"]).unwrap(),
                );
                let mut p = Position::from_map_coordinates(empyrean_entity::Vector2::new(ew, ns));
                let what = format!("{ns}N {ew}E");
                match position_extensions::adjust_map_coords(&mut w, &mut p) {
                    Ok(()) => assert_pos(&p, &case.output, &what),
                    // The harness runs LScape's client path (no LandblockManager array), where ACE's
                    // server throws for landblock row or column 0xFF; none of these points reach it.
                    Err(e) => panic!("{what}: {e}"),
                }
                checked += 1;
            }
            assert!(checked > 150);

            for case in &vectors::load_named("position_real", "adjust_map_coords_buildings").cases {
                let lb = u32::try_from(u64_of(&case.input["landblock"]).unwrap()).unwrap();
                let at = Vector3::new(
                    f32_of(&case.input["x"]).unwrap(),
                    f32_of(&case.input["y"]).unwrap(),
                    f32_of(&case.input["z"]).unwrap(),
                );
                let mut p = Position::from_vectors(lb << 16, at, Quaternion::IDENTITY);
                let what = format!("building of {lb:04X} at {at:?}");
                position_extensions::adjust_map_coords(&mut w, &mut p)
                    .unwrap_or_else(|e| panic!("{what}: {e}"));
                assert_pos(&p, &case.output, &what);
            }
        }

        #[test]
        fn adjust_dungeon_matches_ace_on_the_retail_dats() {
            use empyrean_world::world_objects::world_object;

            let mut w = real_world();
            let file = vectors::load_named("position_real", "adjust_dungeon");
            let mut moved = 0;
            for case in &file.cases {
                let cell = u32::try_from(u64_of(&case.input["cell"]).unwrap()).unwrap();
                let at = Vector3::new(
                    f32_of(&case.input["x"]).unwrap(),
                    f32_of(&case.input["y"]).unwrap(),
                    f32_of(&case.input["z"]).unwrap(),
                );
                let what = format!("0x{cell:08X} at {at:?}");
                let o = &case.output;

                let mut a = Position::from_vectors(cell, at, Quaternion::IDENTITY);
                assert_eq!(
                    world_object::adjust_dungeon_cells(&mut w, &mut a),
                    o["cells_result"].as_bool().unwrap(),
                    "{what}: AdjustDungeonCells"
                );
                assert_pos(&a, &o["cells_pos"], &what);

                let mut b = Position::from_vectors(cell, at, Quaternion::IDENTITY);
                assert_eq!(
                    world_object::adjust_dungeon_pos(&mut w, &mut b),
                    o["pos_result"].as_bool().unwrap(),
                    "{what}: AdjustDungeonPos"
                );
                assert_pos(&b, &o["pos_pos"], &what);

                let mut c = Position::from_vectors(cell, at, Quaternion::IDENTITY);
                world_object::adjust_dungeon(&mut w, &mut c);
                assert_pos(&c, &o["adjust_pos"], &what);
                if c.cell() != cell {
                    moved += 1;
                }
            }
            assert!(
                file.cases.len() > 500 && moved > 400,
                "{moved} of {} moved to their env cell",
                file.cases.len()
            );
        }
    }
}
