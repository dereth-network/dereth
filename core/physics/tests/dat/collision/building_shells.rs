//! Every Holtburg building is a single-part gfxobj with a physics BSP; a sphere driven into a wall
//! is stopped and reports the environment; the shell leaves distant spheres alone; placement takes
//! the insert branch; the shadow-list walk stops a body at a static object.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::collections::HashMap;
use std::sync::Arc;

use dereth_assets::{CellLandblock, Decode, GfxObj, Region};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
use dereth_physics::geom::{Plane, PlaneExt, Polygon, Sphere};
use dereth_physics::transition::collide;
use dereth_physics::transition::spherepath::InsertType;
use dereth_physics::{
    landdefs, BuildingGeometry, LandSource, LandblockCollision, PhysHandle, PhysicsPart,
    PhysicsWorld, SetupGeometry, StaticLandSource, Transition, TransitionState, V3,
};
use dereth_primitives::num::math;
use dereth_primitives::{
    CellId, DataId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3,
};

/// The region the landscape definitions read their variables from.
const REGION_ID: DataId = DataId(0x1300_0000);
const HOLTBURG: LandblockId = LandblockId(0xA9B4);

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

fn convert_bsp(g: &GfxObj) -> Option<BspTree> {
    let src = g.physics_bsp.as_ref()?;
    // A leaf's `in_polys` are polygon *ids*, not indices.
    let mut by_id: HashMap<i16, u32> = HashMap::new();
    let mut polygons = Vec::with_capacity(g.physics_polygons.len());
    for (i, p) in g.physics_polygons.iter().enumerate() {
        let verts: Vec<Vec3> = p
            .vertex_ids
            .iter()
            .map(|&vi| g.vertex_array.vertices[vi as usize].position)
            .collect();
        by_id.insert(p.poly_id, u32::try_from(i).expect("polygon index fits"));
        polygons.push(Polygon::new(verts));
    }
    let mut nodes = Vec::with_capacity(src.nodes.len());
    for n in &src.nodes {
        let kind = if let Some(l) = n.leaf_index {
            BspNodeKind::Leaf {
                leaf_index: i32::try_from(l).unwrap_or(-1),
                solid: n.solid == Some(1),
            }
        } else if n.tag == 0x504F_5254 {
            BspNodeKind::Portal
        } else {
            BspNodeKind::Node
        };
        nodes.push(BspNode {
            sphere: n
                .sphere
                .map_or_else(Sphere::default, |s| Sphere::new(s.center, s.radius)),
            splitting_plane: n.plane.map_or_else(Plane::default, |p| Plane {
                normal: p.normal,
                d: p.d,
            }),
            pos_child: n.pos_child,
            neg_child: n.neg_child,
            kind,
            in_polys: n
                .in_polys
                .iter()
                .filter_map(|id| by_id.get(id).copied())
                .collect(),
        });
    }
    Some(BspTree { nodes, polygons })
}

/// One original-client building, represented with the fields needed by this test.
struct Building {
    gfxobj: DataId,
    /// The land cell containing the building's frame origin, which is
    /// the single runtime land cell that receives `add_building`.
    cell: CellId,
    /// The building's position, also used by part 0 of a simple setup.
    pos: Position,
    part: PhysicsPart,
}

/// Holtburg's landblock collision geometry and its twelve buildings.
fn holtburg(store: &RetailDatStore) -> (StaticLandSource, Vec<Building>) {
    let bytes = store
        .read_typed(DbType::Region, REGION_ID)
        .expect("region 0x13000000");
    let region = Region::decode_payload(REGION_ID, &bytes).expect("the region decodes");
    let table = landdefs::validate_height_table(&region.land_defs.land_height_table)
        .expect("the retail height table passes set_height_table");
    let mut land = StaticLandSource::new(table);

    let did = DataId((u32::from(HOLTBURG.0) << 16) | 0xFFFF);
    let bytes = store
        .read_typed(DbType::LandBlock, did)
        .expect("Holtburg's landblock record");
    let lb = CellLandblock::decode_payload(did, &bytes).expect("the landblock decodes");
    land.add_block(
        LandblockCollision::build(
            HOLTBURG,
            Box::new(lb.height),
            Box::new(lb.terrain),
            lb.lbi_exists != 0,
            8,
            &table,
        )
        .expect("a full-detail block"),
    );

    let lbi_id = DataId((u32::from(HOLTBURG.0) << 16) | 0xFFFE);
    let bytes = store
        .read_typed(DbType::Lbi, lbi_id)
        .expect("Holtburg's LBI");
    let lbi = dereth_assets::world::LandblockInfo::decode_payload(lbi_id, &bytes)
        .expect("the LBI decodes");

    let mut out = Vec::new();
    for b in &lbi.buildings {
        // Adjust the frame origin to an outside cell, resolve that cell,
        // set the initial frame, and register the building with the cell.
        let mut cell = HOLTBURG.cell(1);
        let mut origin = b.frame.origin;
        assert!(
            landdefs::adjust_to_outside(&mut cell, &mut origin),
            "a building inside the block"
        );
        let pos = Position::new(cell, b.frame);
        assert_eq!(b.id.0 >> 24, 0x01, "{:08X} is not a GfxObj id", b.id.0);
        let bytes = store
            .read_typed(DbType::GfxObj, b.id)
            .expect("the building's GfxObj");
        let g = GfxObj::decode_payload(b.id, &bytes).expect("the GfxObj decodes");
        let part = PhysicsPart {
            pos,
            gfxobj_scale: 1.0,
            physics_bsp: convert_bsp(&g).map(Arc::new),
            bound_box: None,
            drawing_sphere: None,
            first_degrade_mode: None,
        };
        out.push(Building {
            gfxobj: b.id,
            cell,
            pos,
            part,
        });
    }
    (land, out)
}

macro_rules! ctx_over {
    ($name:ident) => {
        let land_ = StaticLandSource::linear();
        let objects_ = dereth_physics::arena::Arena::new();
        let cells_ = std::collections::BTreeMap::new();
        let $name = dereth_physics::transition::TransitionCtx {
            land: &land_,
            objects: &objects_,
            cells: &cells_,
            mover: None,
            object_table: None,
            entry_host: None,
        };
    };
    ($name:ident, $land:expr) => {
        let objects_ = dereth_physics::arena::Arena::new();
        let cells_ = std::collections::BTreeMap::new();
        let $name = dereth_physics::transition::TransitionCtx {
            land: $land,
            objects: &objects_,
            cells: &cells_,
            mover: None,
            object_table: None,
            entry_host: None,
        };
    };
}

fn probe(centre: Vec3, movement: Vec3, cell: CellId) -> Transition {
    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
    let from = Position::new(
        cell,
        Frame::new(centre.sub(Vec3::new(0.0, 0.0, 0.5)), Quat::IDENTITY),
    );
    t.sphere_path.init_path(Some(cell), Some(from), &from);
    t.sphere_path.set_check_pos(&from, Some(cell));
    t.sphere_path.add_offset_to_check_pos(movement);
    t
}

// -------------------------------------------------------------------------------------------
// The building shell.
// -------------------------------------------------------------------------------------------

#[test]
fn every_holtburg_building_is_a_single_part_gfxobj_with_a_physics_bsp() {
    let s = store();
    let (_, buildings) = holtburg(&s);
    assert!(!buildings.is_empty(), "the input contains building shells");
    for b in &buildings {
        let tree = b.part.physics_bsp.as_ref().unwrap_or_else(|| {
            panic!(
                "{:08X} has no physics BSP, so its shell would be intangible",
                b.gfxobj.0
            )
        });
        assert!(
            !tree.nodes.is_empty(),
            "{:08X} has an empty tree",
            b.gfxobj.0
        );
        assert!(
            !tree.polygons.is_empty(),
            "{:08X} has no physics polygons",
            b.gfxobj.0
        );
        let sphere = b.part.physics_sphere().expect("the BSP tree's own sphere");
        assert!(
            sphere.radius > 1.0,
            "{:08X}'s root bounding sphere is {} m, too small to be a building",
            b.gfxobj.0,
            sphere.radius
        );
        assert_eq!(
            b.part.gfxobj_scale, 1.0,
            "building a simple setup leaves default_scale NULL"
        );
        assert_eq!(b.part.pos, b.pos);
    }
}

/// The building's own BSP says where its walls are. Walk in along +X from outside the root
/// bounding sphere until the first solid point, in the part's local space.
///
/// Returns `(outside, inside)` in local space: a point in clear space and a point 0.6 m further in
/// that the tree calls solid.
fn a_wall_of(tree: &BspTree, sphere: Sphere) -> Option<(Vec3, Vec3)> {
    // Sample a grid of (y, z) lines through the object and take the first that meets solid space.
    for zi in 0..12_i8 {
        let z = sphere.center.z - sphere.radius * 0.5 + f32::from(zi) * 0.4;
        for yi in -10_i8..=10 {
            let y = sphere.center.y + f32::from(yi) * 0.5;
            let start = sphere.center.x - sphere.radius - 1.0;
            let mut prev: Option<Vec3> = None;
            for xi in 0..600_i16 {
                let x = start + f32::from(xi) * 0.05;
                if x > sphere.center.x {
                    break;
                }
                let p = Vec3::new(x, y, z);
                if tree.point_intersects_solid(p) {
                    // Need 1.6 m of clear run behind it for the mover's own sphere.
                    let back = Vec3::new(x - 1.6, y, z);
                    if prev.is_some() && !tree.point_intersects_solid(back) {
                        return Some((back, p));
                    }
                    break;
                }
                prev = Some(p);
            }
        }
    }
    None
}

#[test]
fn a_sphere_driven_into_a_holtburg_wall_is_stopped_and_the_shell_reports_the_environment() {
    ctx_over!(ctx);
    let s = store();
    let (_, buildings) = holtburg(&s);
    let mut stopped = 0;
    let mut normals_point_back = 0;
    for b in &buildings {
        let tree = b.part.physics_bsp.as_ref().expect("a shell");
        let root = b.part.physics_sphere().expect("a root sphere");
        let Some((outside, inside)) = a_wall_of(tree, root) else {
            continue;
        };
        let m = dereth_physics::math::l2g(b.pos.frame.rotation);
        let world_out = dereth_physics::math::localtoglobalvec(m, outside).add(b.pos.frame.origin);
        let world_in = dereth_physics::math::localtoglobalvec(m, inside).add(b.pos.frame.origin);
        let step = world_in.sub(world_out);

        let building = BuildingGeometry {
            parts: vec![b.part.clone()],
        };

        // The control: the same sphere, the same movement, no building.
        let mut t = probe(world_out, step, b.cell);
        assert_eq!(
            collide::find_building_collisions(&ctx, &mut t, &BuildingGeometry::default()),
            TransitionState::Ok,
            "a building with no part array is never an obstruction"
        );

        let mut t = probe(world_out, step, b.cell);
        let r = collide::find_building_collisions(&ctx, &mut t, &building);
        assert_ne!(
            r,
            TransitionState::Ok,
            "{:08X}: a sphere driven from clear space into the shell was not stopped",
            b.gfxobj.0
        );
        assert!(
            t.collision_info.collided_with_environment,
            "{:08X}: a building is scenery and must report as environment",
            b.gfxobj.0
        );
        assert!(
            !t.sphere_path.bldg_check,
            "the flag is lowered on the way out"
        );
        assert_eq!(t.counters.buildings_bsp_walked, 1);
        assert_eq!(t.counters.building_parts_without_bsp, 0);
        assert_eq!(t.counters.buildings_sphere_pruned, 0);
        assert_eq!(t.counters.buildings_without_parts, 0);
        stopped += 1;

        // The BSP collision walk's ordinary branch records the hit polygon's plane normal
        // rotated into world space. A
        // physics shell's polygons face outward, so it points back the way the sphere came.
        let n = t.sphere_path.step_up_normal;
        if n.dot(step) < 0.0 {
            normals_point_back += 1;
        }
    }
    assert!(
        stopped > 0,
        "only {stopped} of Holtburg's buildings presented a wall to walk into"
    );
    assert_eq!(
        normals_point_back, stopped,
        "a physics shell's polygons face outward, so every collision normal must oppose the \
         approach"
    );
}

#[test]
fn the_shell_leaves_a_sphere_that_never_reaches_it_alone() {
    ctx_over!(ctx);
    let s = store();
    let (_, buildings) = holtburg(&s);
    let mut pruned = 0;
    for b in &buildings {
        let root = b.part.physics_sphere().expect("a root sphere");
        // Two root radii away along +X, moving further away: outside the bounding sphere, so
        // the geometry's bounding-sphere prune answers before the tree is ever entered.
        let m = dereth_physics::math::l2g(b.pos.frame.rotation);
        let away = dereth_physics::math::localtoglobalvec(
            m,
            root.center
                .add(Vec3::new(root.radius * 2.0 + 4.0, 0.0, 0.0)),
        )
        .add(b.pos.frame.origin);
        let mut t = probe(away, Vec3::new(0.1, 0.0, 0.0), b.cell);
        let building = BuildingGeometry {
            parts: vec![b.part.clone()],
        };
        assert_eq!(
            collide::find_building_collisions(&ctx, &mut t, &building),
            TransitionState::Ok
        );
        assert!(!t.collision_info.collided_with_environment);
        assert_eq!(t.counters.buildings_sphere_pruned, 1);
        assert_eq!(t.counters.buildings_bsp_walked, 0);
        pruned += 1;
    }
    assert_eq!(
        pruned,
        buildings.len(),
        "every shell is pruned by the distant sphere"
    );
}

#[test]
fn initial_placement_takes_the_placement_insert_branch_of_the_shell() {
    ctx_over!(ctx);
    // The insert branches on `insert_type` after the prune: the placement insert rather than
    // the collision search. The
    // observable difference is that placement_insert *moves* the spheres out of the geometry and
    // reports `ADJUSTED_TS`, where find_collisions reports the collision.
    let s = store();
    let (_, buildings) = holtburg(&s);
    let mut adjusted_or_collided = 0;
    for b in &buildings {
        let tree = b.part.physics_bsp.as_ref().expect("a shell");
        let root = b.part.physics_sphere().expect("a root sphere");
        let Some((_, inside)) = a_wall_of(tree, root) else {
            continue;
        };
        let m = dereth_physics::math::l2g(b.pos.frame.rotation);
        let world_in = dereth_physics::math::localtoglobalvec(m, inside).add(b.pos.frame.origin);

        let mut t = probe(world_in, Vec3::ZERO, b.cell);
        t.sphere_path.insert_type = InsertType::InitialPlacement;
        let building = BuildingGeometry {
            parts: vec![b.part.clone()],
        };
        let r = collide::find_building_collisions(&ctx, &mut t, &building);
        assert_ne!(
            r,
            TransitionState::Ok,
            "{:08X}: placed inside a wall",
            b.gfxobj.0
        );
        assert!(
            matches!(r, TransitionState::Adjusted | TransitionState::Collided),
            "placement_insert reports one of those two"
        );
        assert_eq!(t.counters.buildings_bsp_walked, 1);
        adjusted_or_collided += 1;
    }
    assert!(adjusted_or_collided > 0);
}

// -------------------------------------------------------------------------------------------
// The whole chain, through `PhysicsWorld`.
// -------------------------------------------------------------------------------------------

fn player_geometry() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.5), 1.0),
        step_up_height: 0.3,
        step_down_height: 0.3,
        radius: 0.5,
        height: 1.0,
        ..SetupGeometry::default()
    })
}

fn spawn(w: &mut PhysicsWorld, id: u32, at: Vec3, per_substep: Vec3) -> PhysHandle {
    let h = w.create(ObjectId(id), player_geometry(), true);
    let mut cell = HOLTBURG.cell(1);
    let mut origin = at;
    landdefs::adjust_to_outside(&mut cell, &mut origin);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            vec![Frame::new(per_substep, Quat::IDENTITY); 400],
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

fn run(w: &mut PhysicsWorld, seconds: f64) {
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    while t < seconds {
        t += dt;
        w.use_time(LocalTime(t), false);
    }
}

/// Would the shell stop a standing body at this block-local `(x, y)` that is trying to move
/// `dir`, and if so which way does the wall face? Asked through the shell itself, so that the
/// scenario is selected by the same swept-sphere test the walk will meet and not by a coarser
/// proxy: `point_intersects_solid` answers "in a leaf that holds geometry", which is a different
/// question.
///
/// The normal comes back from the collision record, where the BSP walk's ordinary branch
/// records the hit polygon's plane normal, rotated
/// into world space.
fn shell_blocks(land: &StaticLandSource, b: &Building, x: f32, y: f32, dir: Vec3) -> Option<Vec3> {
    ctx_over!(ctx, land);
    let gz = ground_at(land, x, y)?;
    let mut cell = HOLTBURG.cell(1);
    let mut o = Vec3::new(x, y, gz);
    if !landdefs::adjust_to_outside(&mut cell, &mut o) || cell != b.cell {
        return None;
    }
    let mut t = probe(Vec3::new(x, y, gz + 0.5), dir, b.cell);
    let building = BuildingGeometry {
        parts: vec![b.part.clone()],
    };
    if collide::find_building_collisions(&ctx, &mut t, &building) == TransitionState::Ok {
        return None;
    }
    Some(t.sphere_path.step_up_normal)
}

/// The terrain height under a block-local `(x, y)`, off the same polygon
///  would stand the body on.
fn ground_at(land: &StaticLandSource, x: f32, y: f32) -> Option<f32> {
    let block = land.landblock(HOLTBURG)?;
    let mut cell = HOLTBURG.cell(1);
    let mut o = Vec3::new(x, y, 0.0);
    if !landdefs::adjust_to_outside(&mut cell, &mut o) {
        return None;
    }
    let poly = block.find_terrain_poly(cell.index(), o)?;
    let mut p = Vec3::new(x, y, 0.0);
    poly.plane.set_height(&mut p).then_some(p.z)
}

struct Approach {
    /// Where the body starts, on Holtburg's own terrain, in block space.
    start: Vec3,
    /// The per-sub-step offset that drives it at the wall, forty degrees off the wall's normal.
    step: Vec3,
}

fn an_approach(land: &StaticLandSource, b: &Building) -> Option<Approach> {
    for k in 0..16_i8 {
        let a = f32::from(k) * std::f32::consts::TAU / 16.0;
        let out = Vec3::new(math::cosf(a), math::sinf(a), 0.0);
        let inward = out.mul(-0.12);
        // March inward until the shell would stop a body walking this way.
        let mut wall = None;
        for i in (0..120_i8).rev() {
            let d = f32::from(i) * 0.1;
            let p = b.pos.frame.origin.add(out.mul(d));
            if let Some(n) = shell_blocks(land, b, p.x, p.y, inward) {
                wall = Some((d, n));
                break;
            }
        }
        let (wall, n) = wall?;
        // A vertical face, pointing back out along the approach. A roof or a doorway lintel is no
        // use to a test about walking into a wall.
        let mut normal = Vec3::new(n.x, n.y, 0.0);
        if normal.normalize_check_small() || normal.dot(out) < 0.85 {
            continue;
        }
        let approach = wall + 2.5;
        if approach > 11.0 {
            continue;
        }
        // Three clear metres of the building's own cell behind the wall.
        let clear = (0..26_i8).all(|i| {
            let q = b
                .pos
                .frame
                .origin
                .add(out.mul(wall + 0.1 + f32::from(i) * 0.1));
            let mut cell = HOLTBURG.cell(1);
            let mut o = q;
            shell_blocks(land, b, q.x, q.y, inward).is_none()
                && landdefs::adjust_to_outside(&mut cell, &mut o)
                && cell == b.cell
        });
        if !clear {
            continue;
        }
        let start = b.pos.frame.origin.add(out.mul(approach));
        let gz = ground_at(land, start.x, start.y)?;
        // Forty degrees off the wall's normal, in the horizontal plane. 0.12 m per 1/30 s sub-step
        // is 3.6 m/s, a walk; four seconds of it covers 14 m.
        let tangent = Vec3::new(-normal.y, normal.x, 0.0);
        let dir = normal.mul(-0.766_044_4).add(tangent.mul(0.642_787_6));
        // The whole oblique corridor has to stay inside the building's own land cell: a body that
        // slides out of it has left the only runtime land cell that knows about this building, and the
        // run stops measuring the shell.
        let start_ground = Vec3::new(start.x, start.y, gz);
        let in_cell = (0..25_i8).all(|i| {
            let q = start_ground.add(dir.mul(f32::from(i) * 0.25));
            let mut cell = HOLTBURG.cell(1);
            let mut o = q;
            landdefs::adjust_to_outside(&mut cell, &mut o) && cell == b.cell
        });
        if !in_cell {
            continue;
        }
        // The scenario is only worth running if the shell stops a body coming in at *this* angle:
        // a sliver of wall that a head-on sweep meets can be missed by an oblique one, and a
        // scenario the shell was never going to affect proves nothing either way.
        let wall_point = b.pos.frame.origin.add(out.mul(wall));
        if shell_blocks(land, b, wall_point.x, wall_point.y, dir.mul(0.12)).is_none() {
            continue;
        }
        return Some(Approach {
            start: start_ground,
            step: dir.mul(0.12),
        });
    }
    None
}

/// Behaviour: physics.buildings.a-body-walking-into-a-building-from-the-street-is-stopped-by-its-wall
#[test]
fn a_body_walking_into_a_holtburg_building_from_the_street_is_stopped_by_its_outer_wall() {
    let s = store();
    let (land, buildings) = holtburg(&s);

    /// Has the body's own collision sphere ended up **inside** the building's geometry? This is
    /// the BSP walk's own solid-sphere placement test with the centre check enabled — asked
    /// of the resting position rather than of a swept move.
    fn inside_shell(b: &Building, p: Vec3) -> bool {
        let m = dereth_physics::math::l2g(b.pos.frame.rotation);
        let centre = p.add(Vec3::new(0.0, 0.0, 0.5));
        let local = dereth_physics::math::globaltolocalvec(m, centre.sub(b.pos.frame.origin));
        b.part
            .physics_bsp
            .as_ref()
            .is_some_and(|t| t.sphere_intersects_solid(&Sphere::new(local, 0.5), true))
    }

    // Where the same body ends up in the same four seconds, with and without the shell registered
    // in the building's own runtime land cell.
    let trial = |b: &Building, a: &Approach, with_shell: bool| -> Vec3 {
        let mut source = holtburg(&s).0;
        if with_shell {
            source.add_building(
                b.cell,
                BuildingGeometry {
                    parts: vec![b.part.clone()],
                },
            );
        }
        let mut w = PhysicsWorld::new(Arc::new(source));
        let h = spawn(&mut w, 1, a.start, a.step);
        run(&mut w, 4.0);
        w.get(h).expect("live").position.frame.origin
    };

    let mut compared = 0;
    for b in &buildings {
        let Some(a) = an_approach(&land, b) else {
            continue;
        };
        assert!(
            !inside_shell(b, a.start),
            "{:08X}: the start is inside the wall",
            b.gfxobj.0
        );
        let end_without = trial(b, &a, false);
        if !inside_shell(b, end_without) {
            // The control never got inside the building at all — terrain, or a doorway, or the
            // corner of the wall decided where it went. Nothing can be concluded from this one.
            continue;
        }
        let end_with = trial(b, &a, true);
        assert!(
            !inside_shell(b, end_with),
            "{:08X}: the body walked through the wall to {end_with:?}; without the shell it \
             reached {end_without:?}",
            b.gfxobj.0
        );
        // It is allowed to *slide*, and it does — what it may not do is get inside.
        compared += 1;
    }
    assert!(
        compared > 0,
        "only {compared} Holtburg buildings gave a usable street-level approach; the test is not \
         measuring anything"
    );
}

/// Oracle: 's last statement, which is
/// `if (r != OK_TS && (object_info.state & 1) == 0) collided_with_environment = 1`.
///
/// A body walking on the ground is `CONTACT`, so hitting a building never *reports* an environment
/// collision — it only stops the body. That is not an oversight in the rebuild: it is the reason
/// walking into a wall is silent while falling onto one is not, and the pair of runs below is the
/// difference stated as a test.
#[test]
fn the_shell_reports_the_environment_only_to_a_mover_that_is_not_already_in_contact() {
    ctx_over!(ctx);
    let s = store();
    let (_, buildings) = holtburg(&s);
    let mut checked = 0;
    for b in &buildings {
        let tree = b.part.physics_bsp.as_ref().expect("a shell");
        let root = b.part.physics_sphere().expect("a root sphere");
        let Some((outside, inside)) = a_wall_of(tree, root) else {
            continue;
        };
        let m = dereth_physics::math::l2g(b.pos.frame.rotation);
        let world_out = dereth_physics::math::localtoglobalvec(m, outside).add(b.pos.frame.origin);
        let world_in = dereth_physics::math::localtoglobalvec(m, inside).add(b.pos.frame.origin);
        let step = world_in.sub(world_out);
        let building = BuildingGeometry {
            parts: vec![b.part.clone()],
        };

        let mut free = probe(world_out, step, b.cell);
        assert_ne!(
            collide::find_building_collisions(&ctx, &mut free, &building),
            TransitionState::Ok
        );
        assert!(free.collision_info.collided_with_environment);

        let mut in_contact = probe(world_out, step, b.cell);
        in_contact.object_info.state |= dereth_physics::transition::ObjectInfoState::CONTACT;
        assert_ne!(
            collide::find_building_collisions(&ctx, &mut in_contact, &building),
            TransitionState::Ok,
            "{:08X}: the wall still stops it",
            b.gfxobj.0
        );
        assert!(
            !in_contact.collision_info.collided_with_environment,
            "{:08X}: an object already in contact must not re-report the environment",
            b.gfxobj.0
        );
        checked += 1;
    }
    assert!(checked > 0);
}

// -------------------------------------------------------------------------------------------
// The object walk, over the same landblock.
// -------------------------------------------------------------------------------------------

#[test]
fn the_shadow_list_walk_stops_a_body_against_a_static_object_and_skips_the_ones_it_must() {
    let s = store();
    let (land, _) = holtburg(&s);
    let block = land.landblock(HOLTBURG).expect("Holtburg is resident");

    // A patch of Holtburg's own terrain, so the ground height is the dat's and not a guess.
    let mut cell = HOLTBURG.cell(1);
    let mut local = Vec3::new(100.0, 100.0, 0.0);
    assert!(landdefs::adjust_to_outside(&mut cell, &mut local));
    let poly = block
        .find_terrain_poly(cell.index(), local)
        .expect("a terrain polygon");
    let mut ground = Vec3::new(100.0, 100.0, 0.0);
    assert!(poly.plane.set_height(&mut ground));

    let mut w = PhysicsWorld::new(Arc::new(holtburg(&s).0));

    // The mover goes into the cell's shadow list first, so that the walk actually reaches its own
    // entry and has to skip it. The client's list is in `add_shadow` order for the same reason.
    let mover = spawn(&mut w, 1, ground, Vec3::ZERO);

    // The obstacle: a static one metre east of the mover, its sphere overlapping the mover's.
    let obstacle = w.create(ObjectId(9), player_geometry(), false);
    w.enter_cell(obstacle, cell);
    w.get_mut(obstacle).expect("live").position = Position::new(
        cell,
        Frame::new(Vec3::new(101.0, 100.0, ground.z), Quat::IDENTITY),
    );
    w.calc_cross_cells(obstacle, false);
    assert!(w.get(obstacle).expect("live").state.is_static());

    // Drive the walk by hand so the counters can be read: `PhysicsWorld` hands its transition back
    // to the pool at the end of every step.
    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
    let from = Position::new(cell, Frame::new(ground, Quat::IDENTITY));
    t.sphere_path.init_path(Some(cell), Some(from), &from);
    t.sphere_path.set_check_pos(&from, Some(cell));
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(0.4, 0.0, 0.0));
    t.object_info.object = Some(mover);

    let ctx = w.transition_ctx(Some(mover));
    assert_eq!(
        ctx.shadow_objects(cell),
        [mover, obstacle],
        "both bodies shadow the same land cell, the mover first"
    );
    let r = collide::cell_find_obj_collisions(&ctx, &mut t, cell);
    assert_ne!(r, TransitionState::Ok, "the static object is solid");
    assert_eq!(
        t.counters.shadow_dangling, 0,
        "a shadow entry named a dead object"
    );
    assert_eq!(
        t.counters.skipped_self, 1,
        "the mover's own shadow is in the same list"
    );
    assert_eq!(t.counters.skipped_parented, 0);
    assert_eq!(
        t.counters.objects_tested, 1,
        "exactly the obstacle was tested"
    );
    assert!(
        t.collision_info.collided_with_environment,
        "a STATIC_PS object reports as environment, not as an object collision"
    );
}
