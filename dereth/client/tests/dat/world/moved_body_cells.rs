//! Which cells a body without a physics mesh is registered in once it has been placed or has
//! moved: the cells its own collision spheres were found in, not the cells its sorting sphere
//! reaches. Another body tests it from exactly those cells, so a creature whose collision sphere
//! reaches over a land-cell line is solid from the cell beyond it.
//!
//! Each walk is a differential: the same body, start and steps twice, once with the creature as
//! its placement registered it and once with the same creature registered again by its sorting
//! sphere. After the walk the body walks back the way it came.
//!
//! A player walking out through a doorway is registered in the land cell outside only once its own
//! spheres are through the doorway, not while its sorting sphere already reaches out: the cells
//! it is registered in are the ones it is drawn by. A player teleported into the doorway is
//! registered where its spheres reach from the start, before it takes a step.
//!
//! Fixture: the retail cell and portal dats (a pile of bones placed where ACE's world database
//! spawns one, and a Holtburg house's front door) through the physics land source with the block
//! and its eight neighbours resident and the block's landscape statics registered. Missing dats
//! fail. No server or GPU.

use std::sync::Arc;

use dereth_assets::{Decode, Setup};
use dereth_client_runtime::world_build::{land_content, read_landblock};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::{
    CellArray, CellResolver, LandSource, PhysHandle, PhysicsWorld, ScriptedMotion, SetupGeometry,
    Sphere, V3,
};
use dereth_primitives::{
    CellId, DataId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3,
};
use dereth_world_data::env_cells::CellStaticObjects;
use dereth_world_data::land_source::DatLandSource;

/// One walking step, 1.5 m/s at one step a frame.
const STEP: f32 = 0.05;
/// A frame a little longer than the 30 Hz minimum quantum, so every frame steps the body once.
const FRAME: f64 = 0.034;

fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

/// The land source with `block` and its eight neighbours resident.
fn resident(store: &Arc<RetailDatStore>, block: u16) -> Arc<DatLandSource> {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let land =
        Arc::new(DatLandSource::new(Arc::clone(store), &region).expect("the retail height table"));
    let (x, y) = (i32::from(block >> 8), i32::from(block & 0xFF));
    for dx in -1..=1 {
        for dy in -1..=1 {
            if let (Ok(bx), Ok(by)) = (u16::try_from(x + dx), u16::try_from(y + dy)) {
                land.load_block_cells(LandblockId((bx << 8) | by));
            }
        }
    }
    land
}

/// A setup's collision half with its parts.
fn geometry(store: &RetailDatStore, id: u32) -> Arc<SetupGeometry> {
    let id = DataId(id);
    let bytes = store.read_typed(DbType::Setup, id).expect("the setup");
    let setup = Setup::decode_payload_in(store.era_of(id), id, &bytes).expect("decodes");
    let mut stats = dereth_world_data::setup::SetupPartStats::default();
    Arc::new(dereth_world_data::setup::setup_geometry_with_parts(
        store, &setup, &mut stats,
    ))
}

/// The world for one block: its landscape statics registered as the client registers them.
fn world_for(store: &Arc<RetailDatStore>, land: &Arc<DatLandSource>, block: u16) -> PhysicsWorld {
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let table = dereth_terrain::land::mesh::height_table(&region);
    let (bx, by) = (i32::from(block >> 8), i32::from(block & 0xFF));
    let lb = read_landblock(store, bx, by).expect("the landblock");
    let mesh = dereth_terrain::land::mesh::generate_landblock_with_table(
        &lb,
        &region,
        &table,
        bx,
        by,
        1,
        dereth_terrain::land::mesh::Direction::InViewerBlock,
    );
    let statics = land_content(store, &region, &lb, &mesh, bx, by, true).land_statics;
    let mut w = PhysicsWorld::new(Arc::clone(land) as Arc<dyn LandSource>);
    CellStaticObjects::new().init(store, &mut w, &statics);
    w
}

/// A body's collision spheres, placed and scaled.
fn spheres_of(w: &PhysicsWorld, h: PhysHandle) -> Vec<Sphere> {
    let o = w.get(h).expect("a live body");
    let m = dereth_physics::math::l2g(o.position.frame.rotation);
    o.geometry
        .path_spheres()
        .iter()
        .map(|s| {
            Sphere::new(
                dereth_physics::math::localtoglobalvec(m, s.center.mul(o.scale))
                    .add(o.position.frame.origin),
                s.radius * o.scale,
            )
        })
        .collect()
}

/// How far apart two sets of spheres are; negative when they overlap, by the overlap.
fn gap(a: &[Sphere], b: &[Sphere]) -> f32 {
    a.iter()
        .flat_map(|x| {
            b.iter()
                .map(move |y| x.center.sub(y.center).mag2().sqrt() - x.radius - y.radius)
        })
        .fold(f32::MAX, f32::min)
}

/// The cells a body is registered in.
fn registered_in(w: &PhysicsWorld, h: PhysHandle) -> Vec<u32> {
    w.get(h)
        .expect("a live body")
        .shadow_objects
        .iter()
        .map(|s| s.cell_id.0)
        .collect()
}

/// What one walk did.
struct Walk {
    /// The closest the body came to the creature's spheres; negative is how far into them.
    closest: f32,
    /// How far the body got back along the way it came.
    walked_back: f32,
}

/// Place a player body at `start` in `cell`, let it settle a second, walk `forward` steps along
/// `dir`, then walk back half as far.
fn walk(
    w: &mut PhysicsWorld,
    player: &Arc<SetupGeometry>,
    creature: PhysHandle,
    (cell, start): (CellId, Vec3),
    dir: Vec3,
    forward: usize,
) -> Walk {
    let target = spheres_of(w, creature);
    let h = w.create(ObjectId(0x5000_0001), Arc::clone(player), true);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(cell, Frame::new(start, Quat::IDENTITY));
        o.set_motion(Box::new(ScriptedMotion::new(
            vec![Frame::default(); 30],
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    w.set_player(h);
    let mut t = 0.0;
    let mut tick = |w: &mut PhysicsWorld| {
        t += FRAME;
        w.use_time(LocalTime(t), false);
    };
    for _ in 0..30 {
        tick(w);
    }
    {
        let o = w.get(h).expect("live");
        assert!(
            o.transient_state.in_contact() && o.position.cell == cell,
            "the body settles on the ground of {:#010X}, not {:#010X}",
            cell.0,
            o.position.cell.0
        );
        assert!(
            gap(&spheres_of(w, h), &target) > 0.0,
            "the body starts clear of the creature"
        );
    }
    let back = forward / 2;
    let mut steps: Vec<Frame> = vec![Frame::new(dir.mul(STEP), Quat::IDENTITY); forward];
    steps.extend(vec![Frame::new(dir.mul(-STEP), Quat::IDENTITY); back]);
    w.get_mut(h)
        .expect("live")
        .set_motion(Box::new(ScriptedMotion::new(steps, true)));
    let mut closest = f32::MAX;
    let mut turned_at = start;
    for i in 0..forward + back {
        tick(w);
        if i < forward {
            closest = closest.min(gap(&spheres_of(w, h), &target));
            turned_at = w.get(h).expect("live").position.frame.origin;
        }
    }
    let end = w.get(h).expect("live").position.frame.origin;
    w.destroy(h);
    let d = end.sub(turned_at);
    Walk {
        closest,
        walked_back: (d.x * d.x + d.y * d.y).sqrt(),
    }
}

/// Behaviour: objects.collision.a-creature-is-solid-from-every-cell-its-collision-spheres-reach
/// A pile of bones (setup `0x020016CE`, ACE's "Noble Remains", one 0.475 m collision sphere at
/// its 0.95 scale and a sorting sphere of no size) placed a few centimetres from the corner of
/// four land cells is registered in all four, the cells its collision sphere reaches. A player
/// walking at it from the cell east of it is stopped at its sphere and walks back. Registered by
/// its sorting sphere instead, which reaches only the cell it stands in, it lets the same walk
/// into its sphere until the player's own spheres reach its cell.
///
/// Fixture: landblock `0x482E`, the pile placed where ACE's world database spawns one, at
/// (47.984, 47.691) in land cell `0x482E000A`; the walk is in land cell `0x482E0012`, heading
/// west from (50.5, 47.4).
#[test]
fn a_creature_whose_sphere_reaches_the_next_land_cell_stops_a_body_walking_from_it() {
    let s = store();
    let block = 0x482E_u16;
    let land = resident(&s, block);
    let mut w = world_for(&s, &land, block);
    let player = geometry(&s, dereth_client_runtime::character::ALUVIAN_MALE_SETUP.0);
    let pile = w.create(ObjectId(0x7000_0001), geometry(&s, 0x0200_16CE), true);
    w.get_mut(pile).expect("live").scale = 0.95;
    let home = CellId(0x482E_000A);
    let at = Position::new(
        home,
        Frame::new(Vec3::new(47.984, 47.691, 10.055), Quat::IDENTITY),
    );
    assert!(w.enter_world(pile, &at), "the pile is placed");
    assert_eq!(w.get(pile).expect("live").cell, Some(home));
    assert_eq!(
        registered_in(&w, pile),
        vec![0x482E_000A, 0x482E_0012, 0x482E_0013, 0x482E_000B],
        "the pile is registered in the four cells its collision sphere reaches"
    );
    let east = CellId(0x482E_0012);
    let start = Vec3::new(50.5, 47.4, 10.4);
    let dir = Vec3::new(-1.0, 0.0, 0.0);
    let by_spheres = walk(&mut w, &player, pile, (east, start), dir, 60);

    w.calc_cross_cells(pile, false);
    assert_eq!(
        registered_in(&w, pile),
        vec![0x482E_000A],
        "its sorting sphere reaches only the cell it stands in"
    );
    let by_sorting_sphere = walk(&mut w, &player, pile, (east, start), dir, 60);

    assert!(
        by_spheres.closest > -0.05,
        "the pile stops the player at its sphere ({:.3} m)",
        by_spheres.closest
    );
    assert!(
        by_sorting_sphere.closest < -0.3,
        "registered by its sorting sphere, the pile lets the player into its sphere ({:.3} m)",
        by_sorting_sphere.closest
    );
    assert!(
        by_spheres.walked_back > 1.0,
        "the player walks back ({:.3} m)",
        by_spheres.walked_back
    );
}

/// The cells a sphere search from `pos` reaches.
fn reached(land: &DatLandSource, pos: &Position, spheres: &[Sphere]) -> Vec<u32> {
    let mut arr = CellArray::new();
    let mut interior = false;
    CellResolver::new(land).find_cell_list(pos, spheres, &mut arr, false, &mut interior);
    arr.cells.iter().map(|c| c.cell_id.0).collect()
}

fn sorted(cells: &[u32]) -> Vec<u32> {
    let mut v = cells.to_vec();
    v.sort_unstable();
    v
}

fn outdoors(cells: &[u32]) -> bool {
    cells.iter().any(|c| c & 0xFFFF < 0x100)
}

/// The front door of a Holtburg house: the room inside it, the door's middle at its sill, and
/// the level direction out through it.
struct Door {
    room: CellId,
    mid: Vec3,
    sill: f32,
    out: Vec3,
}

impl Door {
    /// The point `d` metres out through the door from its middle (inside when negative), 0.3 m
    /// above its sill.
    fn at(&self, d: f32) -> Vec3 {
        Vec3::new(
            self.mid.x + self.out.x * d,
            self.mid.y + self.out.y * d,
            self.sill + 0.3,
        )
    }
}

/// The front door of the house whose front room is `0xA9B4010B`.
fn holtburg_front_door(land: &DatLandSource) -> Door {
    let room = CellId(0xA9B4_010B);
    let cell = land.env_cell(room).expect("the house's front room");
    let door = cell
        .portals
        .iter()
        .find(|p| p.other_cell_id == 0xFFFF_FFFF)
        .expect("the room's door to the outdoors");
    let verts: Vec<Vec3> = door
        .portal
        .vertices
        .iter()
        .map(|v| dereth_physics::math::localtoglobal(&cell.frame, *v))
        .collect();
    #[allow(clippy::cast_precision_loss)]
    let mid = verts
        .iter()
        .fold(Vec3::ZERO, |a, v| a.add(*v))
        .mul(1.0 / verts.len() as f32);
    let sill = verts.iter().map(|v| v.z).fold(f32::MAX, f32::min);
    let n = dereth_physics::math::localtoglobalvec(
        dereth_physics::math::l2g(cell.frame.rotation),
        door.plane().normal,
    );
    let len = (n.x * n.x + n.y * n.y).sqrt();
    let mut out = Vec3::new(n.x / len, n.y / len, 0.0);
    if !door.portal_side {
        out = out.mul(-1.0);
    }
    Door {
        room,
        mid,
        sill,
        out,
    }
}

/// The world-space sorting sphere of a body standing at `at`.
fn sorting_sphere(geometry: &SetupGeometry, at: &Position) -> Sphere {
    let m = dereth_physics::math::l2g(at.frame.rotation);
    Sphere::new(
        dereth_physics::math::localtoglobalvec(m, geometry.sorting_sphere.center)
            .add(at.frame.origin),
        geometry.sorting_sphere.radius,
    )
}

/// Behaviour: physics.cells.a-body-in-a-doorway-is-registered-outdoors-once-its-spheres-are-outside
/// A player walking straight out through the door of a Holtburg house (from its cell `0xA9B4010B`)
/// is registered in the land cell outside, the cell that draws it with the landscape, only from
/// the step its own collision spheres reach out through the doorway. For nine steps before that,
/// 0.45 m of the walk, its sorting sphere already reaches the land cell, and the body is not
/// registered there. At every step it is registered in exactly the cells its collision spheres
/// reach from where it stands.
///
/// Fixture: landblock `0xA9B4`; the walk starts 2.5 m inside the door, level with its sill, and
/// goes 5 m along the door's normal at 1.5 m/s.
#[test]
fn a_player_walking_out_of_a_holtburg_door_is_registered_outdoors_once_its_spheres_are_out() {
    let s = store();
    let block = 0xA9B4_u16;
    let land = resident(&s, block);
    let mut w = world_for(&s, &land, block);
    let player = geometry(&s, dereth_client_runtime::character::ALUVIAN_MALE_SETUP.0);
    let door = holtburg_front_door(&land);
    let (room, out) = (door.room, door.out);
    let start = door.at(-2.5);

    let h = w.create(ObjectId(0x5000_0001), Arc::clone(&player), true);
    assert!(
        w.enter_world(h, &Position::new(room, Frame::new(start, Quat::IDENTITY))),
        "the player is placed inside the door"
    );
    {
        let o = w.get_mut(h).expect("live");
        let mut steps = vec![Frame::default(); 30];
        steps.extend(vec![Frame::new(out.mul(STEP), Quat::IDENTITY); 100]);
        o.set_motion(Box::new(ScriptedMotion::new(steps, true)));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.set_player(h);
    let mut t = 0.0;
    for _ in 0..30 {
        t += FRAME;
        w.use_time(LocalTime(t), false);
    }
    assert!(
        !dereth_physics::landdefs::is_outdoors(w.get(h).expect("live").position.cell),
        "the player settles indoors"
    );
    let (mut sphere_out_first, mut registered_out_first) = (None, None);
    for step in 0..100 {
        t += FRAME;
        w.use_time(LocalTime(t), false);
        let o = w.get(h).expect("live");
        let at = o.position;
        let registered = registered_in(&w, h);
        let sorting = sorting_sphere(&player, &at);
        assert_eq!(
            sorted(&registered),
            sorted(&reached(&land, &at, &spheres_of(&w, h))),
            "at step {step} the player is registered where its collision spheres reach"
        );
        if sphere_out_first.is_none() && outdoors(&reached(&land, &at, &[sorting])) {
            sphere_out_first = Some(step);
        }
        if registered_out_first.is_none() && outdoors(&registered) {
            registered_out_first = Some(step);
        }
    }
    assert!(
        dereth_physics::landdefs::is_outdoors(w.get(h).expect("live").position.cell),
        "the walk ends outside"
    );
    let (sorting, registered) = (
        sphere_out_first.expect("the sorting sphere reaches outdoors"),
        registered_out_first.expect("the player is registered outdoors"),
    );
    assert_eq!(
        registered - sorting,
        9,
        "the player is registered outdoors nine steps after its sorting sphere reaches out \
         (steps {sorting} and {registered})"
    );
}

/// Behaviour: physics.cells.a-teleported-body-is-registered-where-its-placement-found-its-spheres
/// A player teleported to a point in or near a Holtburg house's front doorway is registered in
/// exactly the cells its own collision spheres reach where the placement left it, before it takes
/// a step. At 29 of the 61 points the sorting sphere a moved body used to be placed by reaches
/// other cells than the collision spheres do, so registering the teleported body by that sphere
/// would put it in cells it is not in.
///
/// Fixture: landblock `0xA9B4` with its eight neighbours resident; the player is created in it
/// and teleported to points every 5 cm from 2.5 m inside the door of room `0xA9B4010B` to 0.5 m
/// outside it, each in the room and 0.3 m above the sill.
#[test]
fn a_player_teleported_into_a_holtburg_doorway_is_registered_where_its_spheres_reach() {
    let s = store();
    let region = dereth_world_data::landblock::load_region(&s).expect("the region decodes");
    let mut c = dereth_client_runtime::character::Character::new(&s, &region, 0xA9B4, (96.0, 96.0))
        .expect("the player's body");
    for dx in 0xA8_u16..=0xAA {
        for dy in 0xB3_u16..=0xB5 {
            c.land().load_block_cells(LandblockId((dx << 8) | dy));
        }
    }
    let land = Arc::clone(c.land());
    let door = holtburg_front_door(&land);
    let player = Arc::clone(&c.world.get(c.handle).expect("live").geometry);
    let mut sorting_differs = 0;
    for k in 0..=60_u16 {
        let d = -2.5 + f32::from(k) * 0.05;
        let committed = c.stats.teleports_committed;
        c.teleport(Position::new(
            door.room,
            Frame::new(door.at(d), Quat::IDENTITY),
        ));
        assert_eq!(
            c.stats.teleports_committed,
            committed + 1,
            "the teleport {d:.2} m through the door commits"
        );
        let at = c.world.get(c.handle).expect("live").position;
        let by_spheres = sorted(&reached(&land, &at, &spheres_of(&c.world, c.handle)));
        assert_eq!(
            sorted(&registered_in(&c.world, c.handle)),
            by_spheres,
            "teleported {d:.2} m through the door, the player is registered where its collision \
             spheres reach"
        );
        if sorted(&reached(&land, &at, &[sorting_sphere(&player, &at)])) != by_spheres {
            sorting_differs += 1;
        }
    }
    assert_eq!(
        sorting_differs, 29,
        "at 29 of the 61 points the sorting sphere reaches other cells than the collision spheres"
    );
}
