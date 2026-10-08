//! Where a static object is solid: in the cells it is registered in when it is added to its cell,
//! which a static without a physics mesh or cylinder spheres reaches by its parts' boxes. A body in
//! a cell the object's collision spheres reach into but its boxes do not is not stopped by those
//! spheres until the body itself reaches a cell the object is registered in. A cell its boxes
//! reach and its sorting sphere does not is one it is solid from.
//!
//! Each walk is a differential: the same body, start and steps twice, once with the object
//! registered as the client registers statics and once with the same body re-registered by the
//! moving-object rule, which reaches the next cell by its sorting sphere. After the walk the body
//! walks back the way it came, so a walk that ends inside the object's spheres, or against them,
//! is shown not to leave the body stuck.
//!
//! Fixture: the retail cell and portal dats (a dungeon fire, a scaled piece of landscape scenery
//! and an object on an upper window's sill) through the physics land source with the static's
//! block and its eight neighbours resident. Missing dats fail. No server or GPU.

use std::sync::Arc;

use dereth_assets::{Decode, Setup};
use dereth_client_runtime::world_build::{land_content, read_landblock};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::{
    CellArray, CellResolver, LandSource, PhysHandle, PhysicsWorld, ScriptedMotion, SetupGeometry,
    Sphere, V3,
};
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_world_data::env_cells::{cell_statics, CellStatic, CellStaticObjects, EnvCellLoader};
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
            let (bx, by) = (x + dx, y + dy);
            if let (Ok(bx), Ok(by)) = (u16::try_from(bx), u16::try_from(by)) {
                land.load_block_cells(LandblockId((bx << 8) | by));
            }
        }
    }
    land
}

/// The player's body: the Aluvian male setup's collision spheres.
fn player(store: &RetailDatStore) -> Arc<SetupGeometry> {
    let id = dereth_client_runtime::character::ALUVIAN_MALE_SETUP;
    let bytes = store
        .read_typed(DbType::Setup, id)
        .expect("the player's setup");
    let setup = Setup::decode_payload_in(store.era_of(id), id, &bytes).expect("decodes");
    Arc::new(dereth_world_data::setup::setup_geometry(&setup))
}

/// The collision spheres of a body, placed and scaled, in the space of its own landblock.
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

/// The cells a body is registered (solid) in.
fn solid_in(w: &PhysicsWorld, h: PhysHandle) -> Vec<u32> {
    w.get(h)
        .expect("a live body")
        .shadow_objects
        .iter()
        .map(|s| s.cell_id.0)
        .collect()
}

/// The cells `object`'s own collision spheres reach from its cell.
fn spheres_reach(land: &DatLandSource, w: &PhysicsWorld, object: PhysHandle) -> Vec<u32> {
    let o = w.get(object).expect("a live body");
    let mut arr = CellArray::new();
    arr.do_not_load_cells = true;
    let mut interior = false;
    CellResolver::new(land).find_cell_list(
        &o.position,
        &spheres_of(w, object),
        &mut arr,
        false,
        &mut interior,
    );
    arr.cells.iter().map(|c| c.cell_id.0).collect()
}

/// What one walk did.
struct Walk {
    /// The closest the body came to `object`'s spheres; negative is how far into them it got.
    closest: f32,
    /// How far the body got back along the way it came.
    walked_back: f32,
}

/// Place a body at `start` in `cell`, let it settle a second, walk `forward` steps along `dir`,
/// then walk back half as far.
fn walk(
    w: &mut PhysicsWorld,
    player: &Arc<SetupGeometry>,
    object: PhysHandle,
    (cell, start): (CellId, Vec3),
    dir: Vec3,
    forward: usize,
) -> Walk {
    let target = spheres_of(w, object);
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
            "the body settles on the floor of {:#010X}, not {:#010X}",
            cell.0,
            o.position.cell.0
        );
        assert!(
            gap(&spheres_of(w, h), &target) > 0.0,
            "the body starts clear of the object"
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
        let o = w.get(h).expect("live");
        if o.position.cell.0 >> 16 != cell.0 >> 16 {
            panic!("the walk left the landblock");
        }
        if i < forward {
            closest = closest.min(gap(&spheres_of(w, h), &target));
            turned_at = o.position.frame.origin;
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

/// Place a body at `start` in `cell` with gravity off, move it through the air at `velocity`
/// for `forward` frames, then back at the reverse velocity for half as many. Returns the walk and
/// where the body was when it turned.
fn fly(
    w: &mut PhysicsWorld,
    player: &Arc<SetupGeometry>,
    object: PhysHandle,
    (cell, start): (CellId, Vec3),
    velocity: Vec3,
    forward: usize,
) -> (Walk, Vec3) {
    let target = spheres_of(w, object);
    let h = w.create(ObjectId(0x5000_0001), Arc::clone(player), true);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.state.set_gravity(false);
        o.position = Position::new(cell, Frame::new(start, Quat::IDENTITY));
        o.set_motion(Box::new(ScriptedMotion::new(Vec::new(), true)));
        o.set_velocity(velocity, 0.0);
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    w.set_player(h);
    assert!(
        gap(&spheres_of(w, h), &target) > 0.0,
        "the body starts clear of the object"
    );
    let mut t = 0.0;
    let mut closest = f32::MAX;
    for _ in 0..forward {
        t += FRAME;
        w.use_time(LocalTime(t), false);
        closest = closest.min(gap(&spheres_of(w, h), &target));
    }
    let turned_at = w.get(h).expect("live").position.frame.origin;
    w.get_mut(h)
        .expect("live")
        .set_velocity(velocity.mul(-1.0), t);
    for _ in 0..forward / 2 {
        t += FRAME;
        w.use_time(LocalTime(t), false);
    }
    let o = w.get(h).expect("live");
    assert_eq!(o.position.cell, cell, "the body stays in {:#010X}", cell.0);
    let d = o.position.frame.origin.sub(turned_at);
    w.destroy(h);
    (
        Walk {
            closest,
            walked_back: (d.x * d.x + d.y * d.y).sqrt(),
        },
        turned_at,
    )
}

/// The world as the client builds it for one block's statics: interior cells' statics and the
/// outdoor statics, all added to their cells.
fn world_with(
    store: &Arc<RetailDatStore>,
    land: &Arc<DatLandSource>,
    statics: &[CellStatic],
) -> (PhysicsWorld, Vec<Option<PhysHandle>>) {
    let mut w = PhysicsWorld::new(Arc::clone(land) as Arc<dyn LandSource>);
    let made = CellStaticObjects::new().init(store, &mut w, statics);
    (w, made)
}

/// Behaviour: world.cell-statics.a-static-reaching-into-the-next-cell-does-not-stop-a-body-there
/// A dungeon fire (setup `0x020006F9`) whose 1.27 m collision sphere reaches into the next
/// cell, where its parts' box does not. A body in that cell walking at the fire walks 0.8 m into
/// its sphere before reaching the fire's own cell, and walks back out; registered by the moving
/// rule, the fire would have stopped it at the sphere.
///
/// Fixture: dungeon block `0x03A7`, the fire is static 4 of cell `0x03A70188`; the walk is in cell
/// `0x03A701C0`, through the deepest point a body can reach into the sphere from that cell
/// without its own spheres reaching the fire's cell, heading south-west.
#[test]
fn a_fire_reaching_into_the_next_dungeon_cell_does_not_stop_a_body_in_that_cell() {
    let s = store();
    let block = 0x03A7;
    let land = resident(&s, block);
    let decoded = EnvCellLoader::new().load_block(&s, block);
    let home = decoded
        .iter()
        .find(|d| d.id.0 == 0x03A7_0188)
        .expect("the fire's cell");
    let statics = cell_statics(home);
    assert_eq!(statics[4].id.0, 0x0200_06F9, "static 4 is the fire");
    let player = player(&s);
    let (deep, dir) = (
        Vec3::new(169.955, -104.495, -65.230),
        Vec3::new(
            -std::f32::consts::FRAC_1_SQRT_2,
            -std::f32::consts::FRAC_1_SQRT_2,
            0.0,
        ),
    );
    let next = CellId(0x03A7_01C0);
    let start = deep.sub(dir.mul(2.5)).add(Vec3::new(0.0, 0.0, 0.3));

    let (mut w, made) = world_with(&s, &land, &statics);
    let fire = made[4].expect("the fire has a body");
    assert_eq!(
        solid_in(&w, fire),
        vec![0x03A7_0188],
        "the fire's box stays in its own cell"
    );
    assert!(
        spheres_reach(&land, &w, fire).contains(&next.0),
        "its collision sphere reaches the next cell"
    );
    let static_rule = walk(&mut w, &player, fire, (next, start), dir, 100);

    w.calc_cross_cells(fire, true);
    assert!(
        solid_in(&w, fire).contains(&next.0),
        "the moving rule registers the fire in the next cell by its sorting sphere"
    );
    let moving_rule = walk(&mut w, &player, fire, (next, start), dir, 100);

    assert!(
        static_rule.closest < -0.5,
        "the body walks into the fire's sphere from the next cell ({:.3} m)",
        static_rule.closest
    );
    assert!(
        moving_rule.closest > -0.05,
        "registered in the next cell the fire stops the body at its sphere ({:.3} m)",
        moving_rule.closest
    );
    assert!(
        static_rule.walked_back > 1.0,
        "the body walks back out ({:.3} m)",
        static_rule.walked_back
    );
}

/// Behaviour: world.scenery.an-object-reaching-over-a-land-cell-line-does-not-stop-a-body-on-the-other-side
/// A piece of generated landscape scenery (setup `0x020014A2`, scaled 2.13) whose 4.3 m
/// collision sphere reaches 2 m over the land-cell line, where its unscaled part box does not.
/// A body in the neighbouring
/// land cell walking at the object walks 1.5 m into the sphere, is stopped where its own spheres
/// reach the object's land cell, and walks back; registered by the moving rule, the object would
/// have stopped it at the sphere.
///
/// Fixture: landblock `0xC5F0`, outdoor static 165 (the generated scenery comes first), in land
/// cell `0xC5F00037`; the walk is in land cell `0xC5F0002F`, heading east at the object.
#[test]
fn a_scaled_scenery_object_reaching_over_a_land_cell_line_stops_a_body_only_at_that_line() {
    let s = store();
    let block = 0xC5F0_u16;
    let land = resident(&s, block);
    let region = dereth_world_data::landblock::load_region(&s).expect("the region decodes");
    let table = dereth_terrain::land::mesh::height_table(&region);
    let (bx, by) = (0xC5, 0xF0);
    let lb = read_landblock(&s, bx, by).expect("the landblock");
    let mesh = dereth_terrain::land::mesh::generate_landblock_with_table(
        &lb,
        &region,
        &table,
        bx,
        by,
        1,
        dereth_terrain::land::mesh::Direction::InViewerBlock,
    );
    let content = land_content(&s, &region, &lb, &mesh, bx, by, true);
    let record = content.land_statics[165];
    assert_eq!(
        record.id.0, 0x0200_14A2,
        "outdoor static 165 is the scenery object"
    );
    assert_eq!(record.cell.0, 0xC5F0_0037);
    let player = player(&s);
    let next = CellId(0xC5F0_002F);
    let deep = Vec3::new(143.515, 159.466, 34.434);
    let dir = Vec3::new(1.0, 0.0, 0.0);
    let start = deep.sub(dir.mul(3.0)).add(Vec3::new(0.0, 0.0, 0.3));

    let (mut w, made) = world_with(&s, &land, &content.land_statics);
    let object = made[165].expect("the object has a body");
    assert_eq!(
        solid_in(&w, object),
        vec![0xC5F0_0037],
        "the object's box stays in its own land cell"
    );
    assert!(
        spheres_reach(&land, &w, object).contains(&next.0),
        "its collision sphere reaches the next land cell"
    );
    let static_rule = walk(&mut w, &player, object, (next, start), dir, 110);

    w.calc_cross_cells(object, true);
    assert!(
        solid_in(&w, object).contains(&next.0),
        "the moving rule registers the object in the next land cell by its sorting sphere"
    );
    let moving_rule = walk(&mut w, &player, object, (next, start), dir, 110);

    assert!(
        static_rule.closest < -1.0,
        "the body walks into the object's sphere from the next land cell ({:.3} m)",
        static_rule.closest
    );
    assert!(
        moving_rule.closest > -0.05,
        "registered in the next land cell the object stops the body at its sphere ({:.3} m)",
        moving_rule.closest
    );
    assert!(
        static_rule.walked_back > 1.0,
        "the body walks back out ({:.3} m)",
        static_rule.walked_back
    );
}

/// Behaviour: world.cell-statics.a-static-whose-box-reaches-the-next-cell-stops-a-body-there
/// An object (setup `0x02000624`, one 0.5 m collision sphere, no physics mesh) standing on the
/// sill of an upper-floor window, whose box reaches out through the window into the land cell
/// outside and whose sorting sphere reaches the room behind instead. A body moving through the air
/// outside the window toward it is stopped at its sphere, 0.3 m before the wall stops a body
/// beside it; registered by the moving rule, the object would let the body 0.26 m into its sphere,
/// as far as the wall.
///
/// Fixture: block `0x376A`, the object is static 0 of the window cell `0x376A0104`, 4.6 m above
/// the ground outside. Nothing outside stands at that height, so the body is held at it with
/// gravity off, as a jump or a fall past the window would carry it; it moves east from land cell
/// `0x376A002A` at 1.5 m/s, level with the window, once at the object and once 2 m north of it.
#[test]
fn an_object_on_an_upper_windowsill_stops_a_body_outside_the_window_at_its_sphere() {
    let s = store();
    let block = 0x376A;
    let land = resident(&s, block);
    let decoded = EnvCellLoader::new().load_block(&s, block);
    let all: Vec<CellStatic> = decoded.iter().flat_map(cell_statics).collect();
    let i = all
        .iter()
        .position(|x| x.cell.0 == 0x376A_0104)
        .expect("the window cell holds a static");
    assert_eq!(
        all[i].id.0, 0x0200_0624,
        "static 0 of the window is the object"
    );
    let player = player(&s);
    let outside = CellId(0x376A_002A);
    let at_object = Vec3::new(121.5, 36.02, 43.777);
    let beside = at_object.add(Vec3::new(0.0, 2.0, 0.0));
    let east = Vec3::new(1.5, 0.0, 0.0);

    let (mut w, made) = world_with(&s, &land, &all);
    let object = made[i].expect("the object has a body");
    assert_eq!(
        solid_in(&w, object),
        vec![0x376A_0104, outside.0],
        "the object's box reaches out of the window into the land cell"
    );
    let (static_rule, stopped) = fly(&mut w, &player, object, (outside, at_object), east, 80);
    let (_, wall) = fly(&mut w, &player, object, (outside, beside), east, 80);

    w.calc_cross_cells(object, true);
    assert_eq!(
        solid_in(&w, object),
        vec![0x376A_0104, 0x376A_0101],
        "the moving rule registers the object in the room behind by its sorting sphere"
    );
    let (moving_rule, moving_stopped) =
        fly(&mut w, &player, object, (outside, at_object), east, 80);

    assert!(
        static_rule.closest > -0.05 && static_rule.closest < 0.1,
        "the body is stopped at the object's sphere ({:.3} m)",
        static_rule.closest
    );
    assert!(
        wall.x - stopped.x > 0.2,
        "the object stops the body short of the wall (at {:.3}, the wall at {:.3})",
        stopped.x,
        wall.x
    );
    assert!(
        moving_rule.closest < -0.15 && (moving_stopped.x - wall.x).abs() < 0.05,
        "registered in the room behind, the object lets the body into its sphere ({:.3} m) \
         as far as the wall (at {:.3})",
        moving_rule.closest,
        moving_stopped.x
    );
    assert!(
        static_rule.walked_back > 0.5,
        "the body moves back away ({:.3} m)",
        static_rule.walked_back
    );
}
