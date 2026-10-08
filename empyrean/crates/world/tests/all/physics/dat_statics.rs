//! ACE: Source/ACE.Server/Physics/Common/Landblock.cs::init_static_objs
//! The cell dat's statics get static collision bodies when their landblock loads: room and
//! dungeon objects, landblock objects and generated scenery, placed as ACE's physics landblock
//! places them and registered by the shared static rule. Creatures and missiles the server moves
//! meet them; a player's position is still the one the client asked for. A landblock's bodies go
//! with it, and a static reaching into a landblock loaded after its own is registered there then.
//! The world database's objects stand on them where they are recorded on them, and stand where
//! they are recorded where the statics alone would move or refuse them (V445).
//! Fixture: ACE vectors over the retail dats, the retail dats and `world.pack`, in the
//! real-content tier; a synthetic scene object.

/// A scene object whose base orientation is rolled 10 degrees about its forward axis, turned to
/// a heading: its forward axis stays level and its X axis too, as the client turns it, rather
/// than taking the rolled X axis's tilt as a pitch. (No scene object of the retail data files
/// has such a base orientation.)
/// Divergence: V446
#[test]
fn a_rolled_scene_object_turned_to_a_heading_keeps_its_forward_axis_level() {
    use dereth_primitives::num::math::{atan2f, cosf, sinf};
    use dereth_primitives::{DataId, Frame, Quat, Vec3};
    let half = 10f32.to_radians() / 2.0;
    let obj = dereth_assets::world::ObjectDesc {
        obj_id: DataId(0x0200_07D9),
        base_loc: Frame::new(Vec3::ZERO, Quat::new(cosf(half), 0.0, sinf(half), 0.0)),
        freq: 1.0,
        displace_x: 0.0,
        displace_y: 0.0,
        min_scale: 1.0,
        max_scale: 1.0,
        max_rot: 360.0,
        min_slope: 0.0,
        max_slope: 90.0,
        align: 0,
        orient: 0,
        weenie_obj: 0,
    };
    let q = empyrean_world::physics::phys_ext::rotate_obj(&obj, 1234, 567, 2);
    let m = dereth_primitives::frame::l2g(Quat::new(q.w, q.x, q.y, q.z)).0;
    let heading = atan2f(m[3], m[4]).to_degrees();
    assert!(heading.abs() > 1.0, "turned to a heading: {heading}");
    assert!(m[5].abs() < 1e-6, "the forward axis is level: {}", m[5]);
    assert!(m[2].abs() < 1e-6, "the X axis is level: {}", m[2]);
}

#[cfg(feature = "real-content")]
mod real_content {
    use std::collections::BTreeMap;
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;

    use dereth_physics::{LandSource, PhysHandle};
    use dereth_primitives::frame::V3;
    use dereth_primitives::num::math::{asinf, atan2f};
    use dereth_primitives::{CellId, Frame, LandblockId, Position, Quat, Vec3};
    use empyrean_common::clock::ClockSnapshot;
    use empyrean_common::dotnet::datetime::DotNetDateTime;
    use empyrean_common::vectors;
    use empyrean_dat::{DatManager, RealDats};
    use empyrean_world::physics::phys_ext::{self, DatStatic, DatStaticKind};
    use empyrean_world::World;

    fn dats() -> Arc<DatManager> {
        static DATS: OnceLock<Arc<DatManager>> = OnceLock::new();
        Arc::clone(DATS.get_or_init(|| {
            let dir = dereth_dat::testing::dat_dir();
            let real = RealDats::open(&dir).unwrap_or_else(|e| {
                panic!(
                    "the real-content tier needs the retail dats under {} (DERETH_TEST_DAT_DIR): {e}",
                    dir.display()
                )
            });
            DatManager::initialize(Arc::new(real)).expect("the retail dats initialize")
        }))
    }

    fn real_world() -> World {
        World::new(
            ClockSnapshot {
                portal_year_ticks: 0.0,
                unix_time: 0.0,
                utc: DotNetDateTime::new(2026, 1, 1),
                monotonic: Duration::ZERO,
            },
            dats(),
        )
    }

    /// The landblock and its eight neighbours, the landblock first.
    fn block_and_neighbours(block: u16) -> Vec<u16> {
        let (bx, by) = (i32::from(block >> 8), i32::from(block & 0xFF));
        let mut v = vec![block];
        for dx in -1..=1 {
            for dy in -1..=1 {
                let (x, y) = (bx + dx, by + dy);
                if (dx, dy) != (0, 0) && (0..=0xFE).contains(&x) && (0..=0xFE).contains(&y) {
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    v.push(((x as u16) << 8) | y as u16);
                }
            }
        }
        v
    }

    /// The server's world with `blocks` loaded, and the same world without dat statics: physics
    /// over the same cell dat's land, cells and buildings, given to it directly.
    fn worlds(blocks: &[u16]) -> (World, World) {
        let mut with = real_world();
        let mut without = real_world();
        let land = Arc::new(empyrean_dat::physics::DatLandSource::new(dats()).expect("a region"));
        phys_ext::use_land_source(&mut without, Arc::clone(&land) as Arc<dyn LandSource>);
        for &b in blocks {
            phys_ext::load_landblock(&mut with, b);
            land.load_landblock(LandblockId(b));
            phys_ext::load_landblock(&mut without, b);
        }
        (with, without)
    }

    fn statics_of(w: &World, block: u16, kind: DatStaticKind) -> Vec<DatStatic> {
        phys_ext::landblock_dat_statics(w, block)
            .iter()
            .filter(|s| s.kind == kind)
            .copied()
            .collect()
    }

    fn registered(w: &World, h: PhysHandle) -> Vec<u32> {
        let mut v: Vec<u32> = w
            .physics
            .get(h)
            .expect("a live body")
            .shadow_objects
            .iter()
            .filter(|s| s.cell_present)
            .map(|s| s.cell_id.0)
            .collect();
        v.sort_unstable();
        v
    }

    fn at(cell: u32, x: f32, y: f32, z: f32) -> Position {
        Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY))
    }

    /// A drudge's body (setup `0x020007DD`) placed at `from` walks toward `to` in 0.8 m steps,
    /// each one transition committed as a server-moved creature's is, until it arrives or cannot
    /// move. Answers where it ends.
    fn creature_walk(w: &mut World, from: Position, to: Vec3) -> Vec3 {
        let h = phys_ext::make_object(w, 0x0200_07DD, 0x7100_0001, true);
        assert!(phys_ext::enter_world(w, h, &from), "the drudge is placed");
        for _ in 0..40 {
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
            // No transition: the body cannot move from where it stands.
            let Some(t) = w.physics.transition(h, &cur, &next, false) else {
                break;
            };
            phys_ext::set_position_internal(w, h, &t);
        }
        let end = phys_ext::position(w, h).expect("a body").frame.origin;
        phys_ext::destroy_object(w, h);
        end
    }

    /// A missile-state probe (setup `0x02000124`) flies from `from` to `to` in one transition.
    /// Answers where it ends and whether it met the environment.
    fn missile_flight(w: &mut World, from: Position, to: Position) -> (Vec3, bool) {
        let h = phys_ext::make_object(w, 0x0200_0124, 0x7100_0002, true);
        let state = phys_ext::state(w, h);
        phys_ext::set_state(w, h, state | empyrean_entity::enums::PhysicsState::Missile);
        if let Some(o) = w.physics.get_mut(h) {
            o.cell = Some(from.cell);
            o.position = from;
        }
        let t = w
            .physics
            .transition(h, &from, &to, false)
            .expect("a transition");
        phys_ext::destroy_object(w, h);
        (
            t.sphere_path.curr_pos.frame.origin,
            t.collision_info.collided_with_environment,
        )
    }

    /// A tree's trunk: the first cylinder sphere's low point in landblock space, and its radius,
    /// both at the tree's scale.
    fn trunk(w: &World, s: &DatStatic) -> (Vec3, f32) {
        let o = w.physics.get(s.body).expect("a body");
        let c = o.geometry.cyl_spheres[0];
        (
            dereth_primitives::frame::localtoglobal(&o.position.frame, c.low_pt.mul(o.scale)),
            c.radius * o.scale,
        )
    }

    /// The bodies ACE builds for each sample landblock, block by block (the block, then its
    /// neighbours): the same placements (setup, cell, frame, scale), and the same cells except
    /// where retail's static rule lists a body that ACE's own cell search does not.
    /// Vectors: dat_statics/bodies
    #[test]
    fn each_sample_landblock_gets_the_static_bodies_ace_gives_it() {
        let v = vectors::load_named("dat_statics", "bodies");
        let mut by_block: BTreeMap<u16, Vec<&vectors::Case>> = BTreeMap::new();
        for c in &v.cases {
            let block = u16::try_from(c.input["block"].as_u64().expect("block")).expect("a block");
            by_block.entry(block).or_default().push(c);
        }
        // Where the cells differ, by (block, kind, index): the cells only ACE lists, then the
        // cells only the server lists. ACE never lists a static in another landblock's land
        // cells, follows a portal only into the cells its own cell sees, and finds a part box's
        // land cells from the part's position rather than the cell's (V1).
        let expected: BTreeMap<(u16, &str, u64), (Vec<u32>, Vec<u32>)> = [
            ((0x03A7, "interior", 52), (vec![], vec![0x03A7_018A])),
            ((0x376A, "object", 21), (vec![], vec![0x376A_015A])),
            (
                (0x376A, "object", 36),
                (vec![], vec![0x3769_0028, 0x3769_0030]),
            ),
            ((0xA99E, "scenery", 19), (vec![0xA99E_0019], vec![])),
            ((0xA99E, "scenery", 36), (vec![0xA99E_002D], vec![])),
            ((0xA9B0, "scenery", 1), (vec![], vec![0xA8B0_0039])),
            ((0xA9B0, "scenery", 151), (vec![], vec![0xA9B1_0029])),
            ((0xA9B4, "interior", 95), (vec![], vec![0xA9B4_011C])),
            ((0xA9B4, "interior", 249), (vec![], vec![0xA9B4_0143])),
            ((0xA9B4, "interior", 305), (vec![], vec![0xA9B4_0158])),
            ((0xC5F0, "scenery", 14), (vec![], vec![0xC5F1_0001])),
            ((0xC5F0, "scenery", 33), (vec![], vec![0xC5EF_0010])),
            ((0xC5F0, "scenery", 217), (vec![], vec![0xC6F0_0008])),
        ]
        .into_iter()
        .collect();
        let mut differ = BTreeMap::new();
        let mut bodies = 0;
        for (block, cases) in by_block {
            let mut w = real_world();
            for b in block_and_neighbours(block) {
                phys_ext::load_landblock(&mut w, b);
            }
            for (kind, k) in [
                ("object", DatStaticKind::LandblockObject),
                ("scenery", DatStaticKind::Scenery),
                ("interior", DatStaticKind::Interior),
            ] {
                let ace: Vec<_> = cases.iter().filter(|c| c.input["kind"] == kind).collect();
                let ours = statics_of(&w, block, k);
                assert_eq!(ace.len(), ours.len(), "{block:04X} {kind}: as many bodies");
                for (c, s) in ace.iter().zip(&ours) {
                    let o = &c.output;
                    let index = c.input["index"].as_u64().expect("index");
                    let f = |x: &serde_json::Value| vectors::f32_of(x).expect("a float");
                    let at = format!("{block:04X} {kind} #{index}");
                    assert_eq!(
                        o["id"].as_u64(),
                        Some(u64::from(s.placement.id.0)),
                        "{at}: setup"
                    );
                    assert_eq!(
                        o["cell"].as_u64(),
                        Some(u64::from(s.placement.cell.0)),
                        "{at}: cell"
                    );
                    let (origin, r) = (&o["origin"], &o["rotation"]);
                    let p = s.placement.frame.origin;
                    for (i, ours) in [p.x, p.y, p.z].into_iter().enumerate() {
                        assert!(
                            (f(&origin[i]) - ours).abs() < 3e-4,
                            "{at}: origin {origin} {p:?}"
                        );
                    }
                    let q = s.placement.frame.rotation;
                    assert_eq!(
                        [f(&r[0]), f(&r[1]), f(&r[2]), f(&r[3])],
                        [q.w, q.x, q.y, q.z],
                        "{at}: rotation"
                    );
                    assert!(vectors::same_f32(f(&o["scale"]), s.scale), "{at}: scale");
                    let theirs: Vec<u32> = o["shadows"]
                        .as_array()
                        .expect("cells")
                        .iter()
                        .map(|x| u32::try_from(x.as_u64().expect("a cell")).expect("a cell id"))
                        .collect();
                    let ours = registered(&w, s.body);
                    if theirs != ours {
                        let only_ace = theirs
                            .iter()
                            .filter(|c| !ours.contains(c))
                            .copied()
                            .collect();
                        let only_ours = ours
                            .iter()
                            .filter(|c| !theirs.contains(c))
                            .copied()
                            .collect();
                        differ.insert((block, kind, index), (only_ace, only_ours));
                    }
                    bodies += 1;
                }
            }
        }
        assert_eq!(bodies, v.cases.len(), "every body compared");
        assert_eq!(differ, expected, "the cells that differ from ACE's");
    }

    /// A tree in the middle of a flat land cell south of Holtburg: scenery 26 of `0xA9B3`
    /// (setup `0x02000258`), its trunk, and the ground's height there.
    fn the_tree(w: &World) -> (u32, Vec3, f32, f32) {
        let tree = statics_of(w, 0xA9B3, DatStaticKind::Scenery)[26];
        assert_eq!(tree.placement.id.0, 0x0200_0258, "the tree");
        let (c, r) = trunk(w, &tree);
        let cell = tree.placement.cell.0;
        let ground = w
            .physics
            .terrain_height_at(&at(cell, c.x, c.y, 0.0))
            .expect("land");
        (cell, c, r, ground)
    }

    /// A drudge walking at that tree stops at its trunk; without the tree's body it walks on
    /// through.
    /// ACE: Source/ACE.Server/Physics/Common/Landblock.cs::get_land_scenes
    #[test]
    fn a_creature_walking_into_a_tree_stops_at_its_trunk() {
        let (mut with, mut without) = worlds(&block_and_neighbours(0xA9B3));
        let (cell, c, r, ground) = the_tree(&with);
        let from = at(cell, c.x - r - 3.0, c.y, ground);
        let to = Vec3::new(c.x + r + 3.0, c.y, ground);
        let stopped = creature_walk(&mut with, from, to);
        let gap = c.x - stopped.x;
        assert!(
            gap > r && gap < r + 1.0,
            "stopped against the trunk (radius {r}): {gap} m short of its axis"
        );
        let passed = creature_walk(&mut without, from, to);
        assert!(
            (passed.x - to.x).abs() < 0.01,
            "without the tree's body it walks through: {passed:?}"
        );
    }

    /// A missile flying level at the same tree's trunk, a metre above the ground, meets it as
    /// the environment and stops there; without the tree's body it flies on.
    /// ACE: Source/ACE.Server/Physics/Common/Landblock.cs::get_land_scenes
    #[test]
    fn a_missile_flying_into_a_tree_hits_it_as_the_environment() {
        let (mut with, mut without) = worlds(&block_and_neighbours(0xA9B3));
        let (cell, c, r, ground) = the_tree(&with);
        let from = at(cell, c.x - r - 3.0, c.y, ground + 1.0);
        let to = at(cell, c.x + r + 3.0, c.y, ground + 1.0);
        let (end, hit) = missile_flight(&mut with, from, to);
        assert!(hit, "the missile meets the environment");
        assert!(end.x < c.x - r + 0.01, "it stops at the trunk: {end:?}");
        let (end, hit) = missile_flight(&mut without, from, to);
        assert!(
            !hit && (end.x - to.frame.origin.x).abs() < 0.01,
            "without it, it flies on: {end:?}"
        );
    }

    /// A drudge walking into a Holtburg room's furniture (interior static 172 of `0xA9B4`, a
    /// physics-mesh piece, `0x01000BC7`) stops at it; without its body it walks through.
    /// ACE: Source/ACE.Server/Physics/Common/EnvCell.cs::init_static_objects
    #[test]
    fn a_creature_walking_into_room_furniture_stops_at_it() {
        let (mut with, mut without) = worlds(&block_and_neighbours(0xA9B4));
        let all = phys_ext::landblock_dat_statics(&with, 0xA9B4).to_vec();
        let piece = all[172];
        assert_eq!(
            (piece.kind, piece.placement.id.0),
            (DatStaticKind::Interior, 0x0100_0BC7),
            "the piece"
        );
        let p = piece.placement.frame.origin;
        let cell = piece.placement.cell.0;
        let from = at(cell, p.x + 3.0, p.y, p.z);
        let to = Vec3::new(p.x - 3.0, p.y, p.z);
        let stopped = creature_walk(&mut with, from, to);
        let passed = creature_walk(&mut without, from, to);
        assert!(
            stopped.x > passed.x + 1.0,
            "the furniture stops the drudge: {stopped:?}, without it {passed:?}"
        );
    }

    /// A player's legacy move (`update_object_server`, the client's position forced) from one
    /// side of the same furniture to the other: the server's own transition is stopped by the
    /// furniture, and the player ends where the client asked all the same.
    /// ACE: Source/ACE.Server/Physics/PhysicsObj.cs::update_object_server
    #[test]
    fn a_players_move_through_furniture_ends_where_the_client_asked() {
        use empyrean_common::dotnet::{Quaternion, Vector3};
        use empyrean_entity::enums::PropertyDataId;
        use empyrean_entity::ObjectGuid;
        use empyrean_world::world_objects::kinds::KindData;
        use empyrean_world::world_objects::player_tick;
        use empyrean_world::world_objects::world_object::WorldObject;

        let (mut w, _) = worlds(&block_and_neighbours(0xA9B4));
        let piece = phys_ext::landblock_dat_statics(&w, 0xA9B4)[172];
        let p = piece.placement.frame.origin;
        let cell = piece.placement.cell.0;
        let g = ObjectGuid::new(0x5000_0001);
        let mut o = WorldObject {
            guid: g,
            container: Some(Box::default()),
            creature: Some(Box::default()),
            player: Some(Box::default()),
            kind: KindData::Player,
            ..Default::default()
        };
        // The Aluvian male body.
        o.set_property(PropertyDataId::Setup, 0x0200_0001);
        o.set_location(Some(empyrean_entity::Position::from_components(
            cell,
            p.x + 3.0,
            p.y,
            p.z,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
        o.current_landblock = Some(empyrean_entity::LandblockId::new(cell));
        w.objects.insert(o).expect("fresh guid");
        assert!(
            phys_ext::add_world_object_physics(&mut w, g),
            "the player is placed"
        );
        let h = phys_ext::physics_obj(&w, g).expect("a body");
        let start = phys_ext::position(&w, h).expect("a body");
        let mut request = start;
        request.frame.origin.x = p.x - 3.0;

        let swept = w
            .physics
            .transition(h, &start, &request, false)
            .expect("a transition")
            .sphere_path
            .curr_pos
            .frame
            .origin;
        assert!(
            swept.x > request.frame.origin.x + 1.0,
            "the server's own sweep stops at the furniture: {swept:?}"
        );

        w.now.portal_year_ticks += 0.1;
        player_tick::set_request_pos(
            &mut w,
            g,
            h,
            Vector3::new(
                request.frame.origin.x,
                request.frame.origin.y,
                request.frame.origin.z,
            ),
            Quaternion::IDENTITY,
            Some(request.cell),
            cell,
        );
        assert!(
            player_tick::update_object_server(&mut w, g, h, true),
            "the move succeeds"
        );
        let end = phys_ext::position(&w, h).expect("a body");
        assert_eq!(end.cell, request.cell, "in the cell the client named");
        assert!(
            (end.frame.origin.x - request.frame.origin.x).abs() < 1e-4,
            "at the position the client asked for: {end:?}"
        );
    }

    /// Unloading a landblock destroys its dat statics' bodies and takes them out of every cell;
    /// loading it again builds the same bodies again.
    /// ACE: Source/ACE.Server/Entity/Landblock.cs::Unload
    #[test]
    fn a_landblocks_static_bodies_go_when_it_unloads_and_come_back_when_it_loads() {
        let mut w = real_world();
        phys_ext::load_landblock(&mut w, 0xA9B4);
        let first: Vec<_> = phys_ext::landblock_dat_statics(&w, 0xA9B4)
            .iter()
            .map(|s| {
                (
                    s.kind,
                    s.placement.cell,
                    s.placement.id,
                    registered(&w, s.body),
                )
            })
            .collect();
        assert_eq!(
            first.len(),
            519,
            "Holtburg's 405 room statics and 114 landblock objects"
        );
        assert_eq!(w.physics.body_count(), 519, "one body each");

        phys_ext::unload_landblock(&mut w, 0xA9B4);
        assert!(
            phys_ext::landblock_dat_statics(&w, 0xA9B4).is_empty(),
            "none left"
        );
        assert_eq!(w.physics.body_count(), 0, "every body destroyed");
        assert_eq!(w.physics.cell_shadow_count(), 0, "and out of every cell");

        phys_ext::load_landblock(&mut w, 0xA9B4);
        let again: Vec<_> = phys_ext::landblock_dat_statics(&w, 0xA9B4)
            .iter()
            .map(|s| {
                (
                    s.kind,
                    s.placement.cell,
                    s.placement.id,
                    registered(&w, s.body),
                )
            })
            .collect();
        assert_eq!(again, first, "the same bodies, in the same cells");
    }

    /// A scaled scenery object of `0xC5F0` (index 14, setup `0x020014A1`) reaches a land cell of
    /// `0xC5F1`: loaded alone, its landblock leaves that cell out (it is not loaded); when
    /// `0xC5F1` loads, the object is registered in it, where ACE's lookup of the cell loads the
    /// landblock and registers it at once.
    /// ACE: Source/ACE.Server/Physics/Common/LScape.cs::get_landblock
    #[test]
    fn a_static_reaching_into_a_landblock_loaded_after_its_own_is_registered_there_then() {
        let mut w = real_world();
        phys_ext::load_landblock(&mut w, 0xC5F0);
        let s = statics_of(&w, 0xC5F0, DatStaticKind::Scenery)[14];
        assert_eq!(s.placement.id.0, 0x0200_14A1, "the object");
        assert_eq!(
            registered(&w, s.body),
            vec![0xC5F0_0008],
            "its own cell only"
        );
        let o = w.physics.get(s.body).expect("a body");
        assert!(
            o.shadow_objects
                .iter()
                .any(|x| x.cell_id == CellId(0xC5F1_0001) && !x.cell_present),
            "it reached the unloaded cell"
        );
        let scale = o.scale;

        phys_ext::load_landblock(&mut w, 0xC5F1);
        assert_eq!(
            registered(&w, s.body),
            vec![0xC5F0_0008, 0xC5F1_0001],
            "registered in the neighbour's cell once it loads"
        );
        assert!(
            vectors::same_f32(w.physics.get(s.body).expect("a body").scale, scale),
            "its scale kept"
        );
    }

    /// A scenery body of `0x7209` (index `i` among its scenery, setup `setup`) and the forward
    /// (local Y) and local X axes of its frame, in landblock space.
    fn scenery_axes(w: &World, i: usize, setup: u32) -> (DatStatic, Vec3, Vec3) {
        let s = statics_of(w, 0x7209, DatStaticKind::Scenery)[i];
        assert_eq!(s.placement.id.0, setup, "the object");
        let m = dereth_primitives::frame::l2g(
            w.physics
                .get(s.body)
                .expect("a body")
                .position
                .frame
                .rotation,
        )
        .0;
        (s, Vec3::new(m[3], m[4], m[5]), Vec3::new(m[0], m[1], m[2]))
    }

    /// A scenery rock of `0x7209` (index 92, setup `0x020007D9`, a physics mesh) is generated
    /// from a scene object whose base orientation pitches it up 1.2 degrees. Turned to its
    /// heading, about east, it keeps that pitch, nose up with its X axis level, so its collision
    /// mesh stands as the client draws the rock.
    /// Divergence: V446
    #[test]
    fn a_pitched_scenery_rock_keeps_its_pitch_when_turned_to_its_heading() {
        let mut w = real_world();
        phys_ext::load_landblock(&mut w, 0x7209);
        let (s, forward, x) = scenery_axes(&w, 92, 0x0200_07D9);
        assert!(
            w.physics
                .get(s.body)
                .expect("a body")
                .geometry
                .caches_physics_bsp(),
            "a physics mesh"
        );
        let heading = atan2f(forward.x, forward.y).to_degrees();
        assert!((heading - 87.1).abs() < 0.1, "about east: {heading}");
        let pitch = asinf(forward.z).to_degrees();
        assert!(
            (pitch - 1.2).abs() < 0.01,
            "pitched up 1.2 degrees: {pitch}"
        );
        assert!(x.z.abs() < 1e-5, "no roll: the X axis is level: {x:?}");
    }

    /// A scenery object of `0x7209` with a collision cylinder (index 134, setup `0x02000E71`) is
    /// generated from a scene object whose base orientation pitches it down 1.1 degrees. Turned
    /// to its heading, about north, it keeps that pitch, nose down with its X axis level, rather
    /// than standing level fore and aft and leaning sideways.
    /// Divergence: V446
    #[test]
    fn a_pitched_scenery_cylinder_object_keeps_its_pitch_rather_than_leaning_sideways() {
        let mut w = real_world();
        phys_ext::load_landblock(&mut w, 0x7209);
        let (s, forward, x) = scenery_axes(&w, 134, 0x0200_0E71);
        assert!(
            !w.physics
                .get(s.body)
                .expect("a body")
                .geometry
                .cyl_spheres
                .is_empty(),
            "a collision cylinder"
        );
        let heading = atan2f(forward.x, forward.y).to_degrees();
        assert!(heading.abs() < 1.0, "about north: {heading}");
        let pitch = asinf(forward.z).to_degrees();
        assert!(
            (pitch + 1.1).abs() < 0.01,
            "pitched down 1.1 degrees: {pitch}"
        );
        assert!(x.z.abs() < 1e-5, "no roll: the X axis is level: {x:?}");
    }

    /// A world on the world database (`world.pack`), as the server runs one.
    fn content_world() -> World {
        use empyrean_common::clock::VirtualClock;
        use empyrean_world::entity::timers::TimersState;
        use empyrean_world::managers::{guid_manager, property_manager};
        static PACK: OnceLock<Arc<empyrean_content::PackContent>> = OnceLock::new();
        let pack = PACK.get_or_init(|| {
            let path = empyrean_common::test_paths::world_pack();
            Arc::new(
                empyrean_content::PackContent::open(&path).unwrap_or_else(|e| {
                    panic!(
                    "the real-content tier needs world.pack at {} (EMPYREAN_TEST_WORLD_PACK): {e}",
                    path.display()
                )
                }),
            )
        });
        let clock = VirtualClock::default();
        let timers = TimersState::new(&clock);
        let mut w = World::new(
            ClockSnapshot::take(&clock, timers.portal_year_ticks),
            dats(),
        );
        w.timers = timers;
        w.content = Arc::clone(pack) as _;
        guid_manager::initialize(&mut w, &mut empyrean_testkit::EmptyShard);
        property_manager::initialize(&mut w, true);
        w
    }

    /// The landblock loaded through the landblock manager and ticked once, which creates and
    /// places its world-database objects; the location of `guid` then, if it was created.
    fn created_at(w: &mut World, block: u16, guid: u32) -> Option<empyrean_entity::Position> {
        use empyrean_world::managers::landblock_manager;
        let lb = empyrean_entity::LandblockId::new((u32::from(block) << 16) | 0xFFFF);
        landblock_manager::get_landblock(w, lb, false, false);
        let ticks = w.timers.portal_year_ticks;
        landblock_manager::tick(w, ticks);
        let guid = empyrean_entity::ObjectGuid::new(guid);
        let listed = w
            .landblock_manager
            .landblocks
            .expect(lb)
            .world_object_guids()
            .any(|g| *g == guid);
        listed
            .then(|| w.objects.get(guid).and_then(|o| o.location()))
            .flatten()
    }

    /// Holtburg's portal to the Town Network (wcid 43065) is recorded standing on its dais, a
    /// landblock object of the cell dat 0.26 m above the ground there: placed against the dais's
    /// body it stays where it was recorded, as ACE places it, rather than stepping down to the
    /// ground through the dais.
    /// ACE: Source/ACE.Server/WorldObjects/WorldObject.cs::AddPhysicsObj
    #[test]
    fn a_world_database_portal_recorded_on_its_dais_stays_on_it() {
        let mut w = content_world();
        let at = created_at(&mut w, 0xA9B4, 0x7A9B_4080).expect("the portal is created");
        assert_eq!(at.cell(), 0xA9B4_0003, "in its land cell");
        assert!(
            (at.position_x - 14.3926).abs() < 1e-3 && (at.position_y - 55.6124).abs() < 1e-3,
            "where it was recorded: {at:?}"
        );
        assert!(
            (at.position_z - 78.198).abs() < 1e-3,
            "on the dais, at its recorded height, not the ground's 77.937: {}",
            at.position_z
        );
    }

    /// Where the database records `guid` of `block`, as the server asks the physics to place it.
    fn recorded_at(w: &mut World, block: u16, guid: u32) -> empyrean_entity::Position {
        let i = w
            .content
            .get_cached_instances_by_landblock(block)
            .iter()
            .find(|i| i.guid == guid)
            .cloned()
            .expect("the instance is in the database");
        let mut p = empyrean_entity::Position::from_components(
            i.obj_cell_id,
            i.origin_x,
            i.origin_y,
            i.origin_z,
            i.angles_x,
            i.angles_y,
            i.angles_z,
            i.angles_w,
            false,
        );
        empyrean_world::world_objects::world_object::adjust_dungeon(w, &mut p);
        p
    }

    fn assert_at(at: &empyrean_entity::Position, recorded: &empyrean_entity::Position, what: &str) {
        assert_eq!(at.cell(), recorded.cell(), "{what}: in its recorded cell");
        assert!(
            vectors::same_f32(at.position_x, recorded.position_x)
                && vectors::same_f32(at.position_y, recorded.position_y)
                && vectors::same_f32(at.position_z, recorded.position_z),
            "{what}: at {:?}, recorded at {:?}",
            (at.position_x, at.position_y, at.position_z),
            (
                recorded.position_x,
                recorded.position_y,
                recorded.position_z
            )
        );
    }

    /// Samsur's portal in `0x01AC` is recorded where its body overlaps a room static beside it
    /// (setup `0x020019E3`). An ethereal object still meets a static one, so placed against it the
    /// portal finds no free spot within 4 m: the statics alone refuse it, and it is created where
    /// it is recorded instead.
    /// Divergence: V445
    #[test]
    fn a_world_database_object_recorded_inside_a_static_with_no_room_near_stands_where_recorded() {
        let mut w = content_world();
        let at = created_at(&mut w, 0x01AC, 0x701A_C001).expect("the portal to Samsur is created");
        let recorded = recorded_at(&mut w, 0x01AC, 0x701A_C001);
        assert_at(&at, &recorded, "the portal to Samsur");
    }

    /// Holtburg's portal in `0xBD81` (wcid 1020) is recorded on its stone plinth, a landblock
    /// object of the cell dat with a physics mesh at the portal's own origin: placed against the
    /// plinth it slides 1.33 m off it to the ground, and placed without it it sinks 0.22 m into
    /// the plinth. It stands where it is recorded.
    /// Divergence: V445
    #[test]
    fn a_world_database_portal_recorded_on_its_stone_plinth_stands_on_it() {
        let mut w = content_world();
        let at = created_at(&mut w, 0xBD81, 0x7BD8_1000).expect("the portal is created");
        let recorded = recorded_at(&mut w, 0xBD81, 0x7BD8_1000);
        assert_at(&at, &recorded, "the portal to Holtburg");
    }

    /// The portal to Lost Light (`0x211F`, wcid 1430) is recorded on a static it overlaps: placed
    /// against it the portal slides off, and placed without it the portal sinks 0.07 m into it.
    /// It stands where it is recorded and stays there through ten seconds of ticks, at rest as on
    /// a floor rather than falling from the spot.
    /// Divergence: V445
    #[test]
    fn a_world_database_portal_put_where_it_is_recorded_stays_there() {
        use empyrean_world::managers::landblock_manager;
        let mut w = content_world();
        created_at(&mut w, 0x211F, 0x7211_F000).expect("the portal is created");
        for _ in 0..100 {
            let ticks = empyrean_world::entity::timers::advance_portal_year_ticks(
                &mut w,
                empyrean_common::dotnet::datetime::TimeSpan::from_seconds(0.1),
            );
            w.now.portal_year_ticks = ticks;
            w.now.monotonic += Duration::from_millis(100);
            landblock_manager::tick(&mut w, ticks);
        }
        let at = w
            .objects
            .get(empyrean_entity::ObjectGuid::new(0x7211_F000))
            .and_then(|o| o.location())
            .expect("still in the world");
        let recorded = recorded_at(&mut w, 0x211F, 0x7211_F000);
        assert_at(&at, &recorded, "the portal to Lost Light, ten seconds on");
    }

    /// Samsur's portal to the Town Network (`0x977B`, wcid 43066) is recorded standing on its
    /// dais. The placement sets it down on the dais three hundred-millionths of a metre above its
    /// recorded height: within a tenth of a millimetre counts as where it is recorded, so it keeps
    /// that placement on the dais rather than being put at its record by force.
    /// Divergence: V445
    #[test]
    fn a_world_database_portal_set_down_on_its_dais_a_hair_off_its_record_keeps_that_placement() {
        let mut w = content_world();
        let at = created_at(&mut w, 0x977B, 0x7977_B04E).expect("the portal is created");
        let recorded = recorded_at(&mut w, 0x977B, 0x7977_B04E);
        assert_eq!(at.cell(), recorded.cell(), "in its recorded cell");
        assert!(
            vectors::same_f32(at.position_x, recorded.position_x)
                && vectors::same_f32(at.position_y, recorded.position_y),
            "where it is recorded: {at:?}"
        );
        let above = at.position_z - recorded.position_z;
        assert!(
            above > 0.0 && above < 1e-6,
            "on the dais as the placement sets it, {above} m above its record"
        );
    }

    /// An object of the dungeon `0x0075` (wcid 87601, `0x70075008`) is recorded in one of two
    /// overlapping cells; the placement, with the statics as without them, sets it a hair (4e-8 m)
    /// off its record in the other cell. Within a tenth of a millimetre counts as where it is
    /// recorded, so it keeps the placement and its cell, as ACE places it.
    /// ACE: Source/ACE.Server/WorldObjects/WorldObject.cs::AddPhysicsObj
    #[test]
    fn a_world_database_object_placed_a_hair_off_its_record_in_an_overlapping_cell_keeps_that_cell()
    {
        let mut w = content_world();
        let at = created_at(&mut w, 0x0075, 0x7007_5008).expect("the object is created");
        let recorded = recorded_at(&mut w, 0x0075, 0x7007_5008);
        assert_eq!(
            recorded.cell(),
            (0x0075 << 16) | 0x046D,
            "recorded in one cell"
        );
        assert_eq!(
            at.cell(),
            (0x0075 << 16) | 0x047B,
            "placed in the overlapping one"
        );
        let off = (at.position_z - recorded.position_z).abs();
        assert!(off > 0.0 && off < 1e-6, "a hair off its record: {off} m");
    }

    /// A world-database object the placement moves for a reason of its own, not the statics, is
    /// still moved: Holtburg's jeweller (wcid 716, `0x7A9B4020`) is recorded 0.115 m above the
    /// floor of his shop and steps down onto it, with the statics as without them.
    /// ACE: Source/ACE.Server/WorldObjects/WorldObject.cs::AddPhysicsObj
    #[test]
    fn a_world_database_object_recorded_above_the_floor_still_steps_down_onto_it() {
        let mut w = content_world();
        let at = created_at(&mut w, 0xA9B4, 0x7A9B_4020).expect("the jeweller is created");
        let recorded = recorded_at(&mut w, 0xA9B4, 0x7A9B_4020);
        assert!(
            (recorded.position_z - 66.12).abs() < 1e-3,
            "recorded above the floor: {}",
            recorded.position_z
        );
        assert!(
            (at.position_z - 66.005).abs() < 1e-3,
            "on the floor, not where recorded: {}",
            at.position_z
        );
    }

    /// A landblock object of `0x0605` (index 3, setup `0x0200042B`, with collision cylinders)
    /// stands 1.1 m from the landblock's east edge and reaches across it into `0x0705`. A
    /// world-database object recorded 0.5 m inside `0x0705` there (a portal, wcid 1026, given a
    /// static guid: the world database records none at that spot) overlaps it, and placed against
    /// it would slide 2.7 m: the neighbour's static alone moves it, so it stands where it is
    /// recorded, as one overlapping a static of its own landblock does.
    /// Divergence: V445
    #[test]
    fn a_world_database_object_recorded_where_a_neighbouring_landblocks_static_reaches_stands_there(
    ) {
        use empyrean_world::factories::world_object_factory as factory;
        use empyrean_world::managers::landblock_manager;
        let mut w = content_world();
        phys_ext::load_landblock(&mut w, 0x0605);
        let lb = empyrean_entity::LandblockId::new(0x0705_FFFF);
        landblock_manager::get_landblock(&mut w, lb, false, false);
        let ticks = w.timers.portal_year_ticks;
        landblock_manager::tick(&mut w, ticks);
        let reaching = phys_ext::landblock_dat_statics(&w, 0x0605)[3];
        assert_eq!(
            (reaching.kind, reaching.placement.id.0),
            (DatStaticKind::LandblockObject, 0x0200_042B),
            "the landblock object"
        );
        assert!(
            registered(&w, reaching.body).contains(&0x0705_0001),
            "it reaches into 0x0705's corner cell"
        );
        let z = w
            .physics
            .terrain_height_at(&at(0x0705_0001, 0.5, 2.5, 0.0))
            .expect("land");
        let recorded = empyrean_entity::Position::from_components(
            0x0705_0001,
            0.5,
            2.5,
            z,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        );
        let mut o = factory::create_new_world_object_by_wcid_in_world(&mut w, 1026)
            .expect("the portal's weenie");
        o.guid = empyrean_entity::ObjectGuid::new(0x7070_5FFE);
        assert!(o.guid.is_static(), "an object of the world database");
        o.set_location(Some(recorded));
        let guid = o.guid;
        w.objects.insert(o).expect("fresh guid");
        assert!(
            empyrean_world::dispatch::enter_world::enter_world(&mut w, guid),
            "the portal is created"
        );
        let placed = w
            .objects
            .get(guid)
            .and_then(|o| o.location())
            .expect("in the world");
        assert_at(
            &placed,
            &recorded,
            "the portal across from the landblock object",
        );
    }

    /// Only the world database's own objects stand where they are recorded: the same portal made
    /// as a new object (a dynamic guid) at Samsur's portal's recorded position is placed against
    /// the room static as ACE places it, and refused.
    /// ACE: Source/ACE.Server/WorldObjects/WorldObject.cs::AddPhysicsObj
    #[test]
    fn an_object_made_in_play_inside_a_static_with_no_room_near_is_not_created() {
        use empyrean_world::factories::world_object_factory as factory;
        let mut w = content_world();
        created_at(&mut w, 0x01AC, 0x701A_C001).expect("the database's portal is created");
        let recorded = recorded_at(&mut w, 0x01AC, 0x701A_C001);
        let mut o = factory::create_new_world_object_by_wcid_in_world(&mut w, 1026)
            .expect("the portal's weenie");
        assert!(o.guid.is_dynamic(), "a new object");
        o.set_location(Some(recorded));
        let guid = o.guid;
        w.objects.insert(o).expect("fresh guid");
        assert!(
            !empyrean_world::dispatch::enter_world::enter_world(&mut w, guid),
            "the new portal is refused"
        );
    }
}
