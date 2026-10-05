//! A registered building shell, and a recorded object, stop a walking body; without registration
//! the same walk passes through. Each walk is a differential: the same terrain, body and script
//! twice, once with the obstacle registered and once without (`NoBuildings` hides the shells while
//! forwarding terrain and interior data). A carried object has no body, and a dropped one gains it.
//! Walls are approached forty degrees off their normal, as in `core/physics/tests/dat/collision/building_shells.rs`.
//!
//! Fixture: Holtburg's twelve `BuildInfo` records, their physics BSPs and terrain from the retail
//! dats, and `first-login-walk-jump` replayed from `fixtures/packet-captures` for the recorded
//! objects. Missing dats or recordings fail. No server or GPU.

use crate::common::captures_dir_opt as captures_dir;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use dereth_assets::{Decode, GfxObj};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::source::{BuildingGeometry, EnvCellGeometry};
use dereth_physics::transition::collide;
use dereth_physics::{
    landdefs, LandSource, LandblockCollision, PhysHandle, PhysicsWorld, SetupGeometry, Sphere,
    Transition, TransitionState, V3,
};
use dereth_primitives::num::math;
use dereth_primitives::{
    CellId, DataId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3,
};
use dereth_world_data::land_source::DatLandSource;

/// Holtburg, whose landblock-info record supplies the twelve building entries checked below.
const HOLTBURG: LandblockId = LandblockId(0xA9B4);

/// Open the required retail store or fail: a missing install is a failure, never a skip.
fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

fn land(store: &Arc<RetailDatStore>) -> Arc<DatLandSource> {
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    Arc::new(DatLandSource::new(Arc::clone(store), &region).expect("the retail height table"))
}

/// Suppress shell lookup while forwarding the same landblocks, height table, interior cells
/// and building-portal cell lists. The wrapper uses the trait's default residency query rather
/// than forwarding the source's window query; the differential runs inside preloaded Holtburg.
#[derive(Debug)]
struct NoBuildings(Arc<DatLandSource>);

impl LandSource for NoBuildings {
    fn landblock(&self, id: LandblockId) -> Option<Arc<LandblockCollision>> {
        self.0.landblock(id)
    }
    fn height_table(&self) -> &[f32; dereth_physics::globals::LAND_HEIGHT_TABLE_LEN] {
        self.0.height_table()
    }
    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
        self.0.env_cell(cell)
    }
    fn building_cells(&self, cell: CellId) -> Vec<CellId> {
        self.0.building_cells(cell)
    }
    fn building(&self, _cell: CellId) -> Option<Arc<BuildingGeometry>> {
        None
    }
}

// ---------------------------------------------------------------------------------------------
// 1. The building shells: `DatLandSource::building`.
// ---------------------------------------------------------------------------------------------

/// Decode Holtburg's landblock info again. Adjust a copy of each building origin to choose
/// its outdoor cell, but keep the unadjusted frame as the part's position. This checks placement
/// against the input record rather than a self-consistent transposed or cell-local result.
/// The decoder and coordinate helper are shared with production. A cell keeps its first offered
/// building; later offers count toward the census but do not replace its geometry.
#[test]
fn every_holtburg_building_reaches_the_land_cell_its_origin_falls_in_at_its_own_frame() {
    let s = store();
    let src = land(&s);
    src.load_block_cells(HOLTBURG);

    let lbi_id = dereth_client_runtime::landblock::lbi_did(HOLTBURG.0);
    let bytes = s.read_typed(DbType::Lbi, lbi_id).expect("Holtburg's LBI");
    let lbi = dereth_assets::world::LandblockInfo::decode_payload(lbi_id, &bytes).expect("decodes");
    assert_eq!(
        lbi.buildings.len(),
        12,
        "Holtburg's LBI carries twelve BuildInfo entries"
    );

    let st = src.building_stats();
    assert_eq!(st.buildings, 12);
    assert_eq!(st.not_a_gfxobj, 0, "a BuildInfo id that is not a GfxObj id");
    assert_eq!(
        st.undecodable, 0,
        "a building's graphics-object record would not decode"
    );
    assert_eq!(
        st.without_bsp, 0,
        "a building whose shell has no physics BSP is intangible"
    );
    assert_eq!(
        st.outside_the_block, 0,
        "adjust_to_outside refused a building's origin"
    );
    assert_eq!(
        u64::try_from(src.resident_buildings()).expect("small"),
        st.registered,
        "every registered shell is reachable through LandSource::building"
    );
    assert_eq!(
        st.registered + st.second_in_a_cell,
        12,
        "every BuildInfo was either added to a spatial sort cell or dropped by add_building"
    );

    let mut seen: BTreeMap<u32, u32> = BTreeMap::new();
    for b in &lbi.buildings {
        assert_eq!(b.id.0 >> 24, 0x01, "{:#010X} is not a GfxObj id", b.id.0);
        let mut cell = HOLTBURG.cell(1);
        let mut origin = b.frame.origin;
        assert!(
            landdefs::adjust_to_outside(&mut cell, &mut origin),
            "inside the block"
        );
        let n = seen.entry(cell.0).or_default();
        *n += 1;
        let g = src
            .building(cell)
            .expect("the spatial sort cell holds a building");
        assert_eq!(
            g.parts.len(),
            1,
            "a graphics-object building shell has one part"
        );
        let part = &g.parts[0];
        if *n > 1 {
            // The cell keeps its first offered building. A later record sharing this cell is
            // counted above, but its frame/tree cannot be compared to the retained first one.
            continue;
        }
        assert_eq!(
            part.pos,
            Position::new(cell, b.frame),
            "{:#010X}: part 0 is at the building's own un-adjusted frame",
            b.id.0
        );
        assert_eq!(
            part.gfxobj_scale, 1.0,
            "the building part has unit graphics-object scale"
        );

        // Decode and convert the same GfxObj again, comparing counts and root radius below;
        // this does not compare every node or polygon value independently.
        let bytes = s
            .read_typed(DbType::GfxObj, b.id)
            .expect("the building's GfxObj");
        let gfx = GfxObj::decode_payload(b.id, &bytes).expect("the GfxObj decodes");
        let want = dereth_world_data::env_cells::gfxobj_physics_bsp(&gfx).expect("a physics BSP");
        let got = part
            .physics_bsp
            .as_ref()
            .expect("the shell carries its tree");
        assert_eq!(
            got.nodes.len(),
            want.nodes.len(),
            "{:#010X}: node count",
            b.id.0
        );
        assert_eq!(
            got.polygons.len(),
            gfx.physics_polygons.len(),
            "{:#010X}",
            b.id.0
        );
        assert_eq!(
            got.root().map(|n| n.sphere.radius),
            want.root().map(|n| n.sphere.radius),
            "{:#010X}: collision-tree root-sphere radius matches the decoded record",
            b.id.0
        );
        assert!(
            part.physics_sphere().is_some_and(|s| s.radius > 1.0),
            "{:#010X}: a building's root sphere is metres across",
            b.id.0
        );
    }
    // A land cell without a registered building returns None, so shell collision discovery
    // has no building tree to test there.
    let empty = (1..=0x40_u32)
        .map(|i| CellId((u32::from(HOLTBURG.0) << 16) | i))
        .find(|c| !seen.contains_key(&c.0))
        .expect("Holtburg has land cells with no building");
    assert!(src.building(empty).is_none());
    // Building lookup, like interior-cell lookup, is resident-only and must not load an
    // unprefetched block from disk.
    assert!(
        src.building(CellId(0xAAB4_0001)).is_none(),
        "building() loaded from disk"
    );
}

/// Sample 4,096 landblock IDs, using coordinates 0..=0xFC in steps of four on each axis.
/// The same loader used by the client must find more than 400 building records, all naming
/// graphics objects (high byte 0x01), with no decode/BSP/outside-block failures and more than
/// 80% registered. This is a spaced world sample, not every building in the archive.
///
/// A graphics-object shell has one identity local part frame, so placement uses the building
/// frame without an additional setup-part composition. A setup ID (high byte 0x02), or any
/// other non-graphics-object ID, is counted and refused rather than placed by that shortcut.
#[test]
fn no_retail_building_in_a_world_wide_sample_is_a_setup_id() {
    let s = store();
    let src = land(&s);
    let mut x = 0_u16;
    while x <= 0xFC {
        let mut y = 0_u16;
        while y <= 0xFC {
            src.load_block_cells(LandblockId((x << 8) | y));
            y += 4;
        }
        x += 4;
    }
    let st = src.building_stats();
    assert!(
        st.buildings > 400,
        "only {} buildings in the sample",
        st.buildings
    );
    assert_eq!(
        st.not_a_gfxobj, 0,
        "a sampled BuildInfo does not name a GfxObj record"
    );
    assert_eq!(
        st.undecodable, 0,
        "a building's graphics-object record is missing or will not decode"
    );
    assert_eq!(
        st.without_bsp, 0,
        "a building whose shell carries no physics BSP"
    );
    assert_eq!(st.outside_the_block, 0);
    assert!(
        st.registered * 10 > st.buildings * 8,
        "{} of {} shells registered",
        st.registered,
        st.buildings
    );
}

// ---------------------------------------------------------------------------------------------
// The walk. Shared with `core/physics/tests/dat/collision/building_shells.rs`, over `DatLandSource` instead of a
// hand-built `StaticLandSource`.
// ---------------------------------------------------------------------------------------------

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

/// A body at `at` (block-local, Holtburg) walking `per_substep` every animation frame.
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

/// A 0.5 m sphere standing at `centre` in Holtburg's block space, asked to move by `movement`.
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

/// Would the shell in `cell` stop a standing body at `(x, y)` moving `dir`, and which way does the
/// wall face? Asked through the shell itself, so the scenario is chosen by the same swept-sphere
/// test the walk will meet.
fn shell_blocks(src: &DatLandSource, cell: CellId, x: f32, y: f32, dir: Vec3) -> Option<Vec3> {
    let gz = src.ground_height(HOLTBURG, x, y)?;
    let mut here = HOLTBURG.cell(1);
    let mut o = Vec3::new(x, y, gz);
    if !landdefs::adjust_to_outside(&mut here, &mut o) || here != cell {
        return None;
    }
    let g = src.building(cell)?;
    let mut t = probe(Vec3::new(x, y, gz + 0.5), dir, cell);
    // Shell stepping runs the BSP sphere-step path, which requires a transition context. This direct single-sphere/part probe supplies empty object and cell-runtime
    // collections; it is not a populated-arena collision test.
    let objects = dereth_physics::arena::Arena::new();
    let cells = std::collections::BTreeMap::new();
    let ctx = dereth_physics::transition::TransitionCtx {
        land: src,
        objects: &objects,
        cells: &cells,
        mover: None,
        object_table: None,
        entry_host: None,
    };
    if collide::find_building_collisions(&ctx, &mut t, &g) == TransitionState::Ok {
        return None;
    }
    Some(t.sphere_path.step_up_normal)
}

struct Approach {
    start: Vec3,
    step: Vec3,
}

/// A street-level approach into one building's wall, forty degrees off its normal. See the module
/// note for why oblique, and `core/physics/tests/dat/collision/building_shells.rs::an_approach`, which this follows.
fn an_approach(src: &DatLandSource, cell: CellId, origin: Vec3) -> Option<Approach> {
    for k in 0..16_i8 {
        let a = f32::from(k) * std::f32::consts::TAU / 16.0;
        let out = Vec3::new(math::cosf(a), math::sinf(a), 0.0);
        let inward = out.mul(-0.12);
        let mut wall = None;
        for i in (0..120_i8).rev() {
            let d = f32::from(i) * 0.1;
            let p = origin.add(out.mul(d));
            if let Some(n) = shell_blocks(src, cell, p.x, p.y, inward) {
                wall = Some((d, n));
                break;
            }
        }
        let (wall, n) = wall?;
        let mut normal = Vec3::new(n.x, n.y, 0.0);
        if normal.normalize_check_small() || normal.dot(out) < 0.85 {
            continue;
        }
        let approach = wall + 2.5;
        if approach > 11.0 {
            continue;
        }
        let clear = (0..26_i8).all(|i| {
            let q = origin.add(out.mul(wall + 0.1 + f32::from(i) * 0.1));
            let mut here = HOLTBURG.cell(1);
            let mut o = q;
            shell_blocks(src, cell, q.x, q.y, inward).is_none()
                && landdefs::adjust_to_outside(&mut here, &mut o)
                && here == cell
        });
        if !clear {
            continue;
        }
        let start = origin.add(out.mul(approach));
        let gz = src.ground_height(HOLTBURG, start.x, start.y)?;
        let tangent = Vec3::new(-normal.y, normal.x, 0.0);
        let dir = normal.mul(-0.766_044_4).add(tangent.mul(0.642_787_6));
        let start_ground = Vec3::new(start.x, start.y, gz);
        let in_cell = (0..25_i8).all(|i| {
            let q = start_ground.add(dir.mul(f32::from(i) * 0.25));
            let mut here = HOLTBURG.cell(1);
            let mut o = q;
            landdefs::adjust_to_outside(&mut here, &mut o) && here == cell
        });
        if !in_cell {
            continue;
        }
        let wall_point = origin.add(out.mul(wall));
        if shell_blocks(src, cell, wall_point.x, wall_point.y, dir.mul(0.12)).is_none() {
            continue;
        }
        return Some(Approach {
            start: start_ground,
            step: dir.mul(0.12),
        });
    }
    None
}

/// Behaviour: world.collision.a-building-shell-stops-a-body-only-when-registered
/// Compare a scripted body over the same land source with shell lookup enabled and disabled.
/// The fixture tests the building wall, not a movable door. Approach selection uses the same
/// shell collision routines as the walk, so it is not an independent geometry derivation.
///
/// Only retained first-in-cell buildings with usable approaches are tried. The unregistered
/// result must intersect the shell; otherwise the station skips that building. The registered
/// result must not intersect it. At least three buildings must produce this differential.
#[test]
fn a_body_walking_into_a_holtburg_wall_is_stopped_only_when_the_client_registers_the_shell() {
    let s = store();
    let src = land(&s);
    src.load_block_cells(HOLTBURG);

    let lbi_id = dereth_client_runtime::landblock::lbi_did(HOLTBURG.0);
    let bytes = s.read_typed(DbType::Lbi, lbi_id).expect("Holtburg's LBI");
    let lbi = dereth_assets::world::LandblockInfo::decode_payload(lbi_id, &bytes).expect("decodes");

    /// Transform the final body's 0.5 m sphere into shell-local space and test intersection
    /// with the physics BSP, including the centre check. This is not just point containment.
    fn inside_shell(g: &BuildingGeometry, p: Vec3) -> bool {
        let part = &g.parts[0];
        let m = dereth_physics::math::l2g(part.pos.frame.rotation);
        let centre = p.add(Vec3::new(0.0, 0.0, 0.5));
        let local = dereth_physics::math::globaltolocalvec(m, centre.sub(part.pos.frame.origin));
        part.physics_bsp
            .as_ref()
            .is_some_and(|t| t.sphere_intersects_solid(&Sphere::new(local, 0.5), true))
    }

    let mut compared = 0;
    for b in &lbi.buildings {
        let mut cell = HOLTBURG.cell(1);
        let mut origin = b.frame.origin;
        if !landdefs::adjust_to_outside(&mut cell, &mut origin) {
            continue;
        }
        let Some(g) = src.building(cell) else {
            continue;
        };
        // Only compare the building actually retained by the first-offer rule.
        if g.parts[0].pos != Position::new(cell, b.frame) {
            continue;
        }
        let Some(a) = an_approach(&src, cell, b.frame.origin) else {
            continue;
        };
        assert!(
            !inside_shell(&g, a.start),
            "{:#010X}: the start is inside the wall",
            b.id.0
        );

        let trial = |registered: bool| -> Vec3 {
            let source: Arc<dyn LandSource> = if registered {
                Arc::clone(&src) as Arc<dyn LandSource>
            } else {
                Arc::new(NoBuildings(Arc::clone(&src)))
            };
            let mut w = PhysicsWorld::new(source);
            let h = spawn(&mut w, 1, a.start, a.step);
            run(&mut w, 4.0);
            w.get(h).expect("live").position.frame.origin
        };

        let without = trial(false);
        if !inside_shell(&g, without) {
            // The control never got inside at all — terrain, a doorway or the corner of the wall
            // decided where it went, and nothing can be concluded from this building.
            continue;
        }
        let with = trial(true);
        assert!(
            !inside_shell(&g, with),
            "{:#010X}: the body walked through the wall to {with:?}; unregistered it reached \
             {without:?}",
            b.id.0
        );
        compared += 1;
    }
    assert!(
        compared >= 3,
        "only {compared} Holtburg buildings gave a usable street-level approach; at least three are required"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The objects, from the recorded corpus.
// ---------------------------------------------------------------------------------------------

use dereth_client_net::client_session::testing::capture::{self, Datagram as Record};

fn load(session: &str) -> Vec<Record> {
    capture::shared_session(session).to_vec()
}

/// The recorded connection sequence from the LoginRequest, so replay setup matches this
/// recording rather than inventing a fresh one.
fn connection_sequence_number(records: &[Record]) -> u32 {
    dereth_client_net::recording::connection_sequence_number(records)
        .expect("the capture has no LoginRequest")
}

fn addr(pair: u16) -> SocketAddr {
    capture::peer(pair)
}

/// Replay incoming datagrams through transport, session and object processing, ticking time
/// and draining outgoing data locally. On the first character set, select its first character.
/// Return the final stream plus the index of the last row at which that stream was nonempty.
///
/// This index is not a maximum-population measurement. The callers first replay to completion,
/// then stop a second run after that last nonempty row to retain a populated pre-logout state.
fn replay(session: &str, limit: usize) -> (ObjectStream, usize) {
    let records = load(session);
    assert!(!records.is_empty(), "{session} is empty");
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut last_populated = 0usize;

    for (index, r) in records.iter().enumerate() {
        if index >= limit {
            break;
        }
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        if !objects.is_empty() {
            last_populated = index;
        }
    }
    (objects, last_populated)
}

/// Require the committed capture corpus instead of treating absence as a successful early
/// return. The individual session loader separately requires its file and valid records.
fn require_captures() {
    assert!(
        captures_dir().is_some(),
        "the capture corpus under fixtures/packet-captures is this file's oracle"
    );
}

/// Synchronize first-login-walk-jump's last nonempty state and require one body for each positioned
/// non-player presence counted by this fixture, with no setup/placement failures. Direct
/// transitions then inspect the player's block's 64 outdoor cells: dangling-shadow and
/// missing-building-BSP counters must stay zero, with positive visited/tested totals.
/// Those totals establish exercised traversal, not that each body's identity was visited.
///
/// Original shadow lists held pointers; the current world uses arena handles. A nonzero
/// dangling counter indicates a stale handle, while the missing-BSP counter indicates a shell
/// without collision geometry. The logout half reuses the populated physics mapping against
/// the completed stream and requires destruction of all bodies and removal of every shadow.
#[test]
fn the_corpus_objects_become_solid_and_the_cell_bookkeeping_stays_clean() {
    require_captures();
    let s = store();
    let (_, populated) = replay("first-login-walk-jump", usize::MAX);
    let (mut stream, _) = replay("first-login-walk-jump", populated + 1);
    let player = stream.player().expect("the corpus reaches the world");
    let pos = stream
        .presence(player)
        .and_then(|p| p.position)
        .expect("the player is placed");
    let block = pos.cell.landblock();

    let src = land(&s);
    src.load_block_cells(block);
    let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
    stream.sync_physics(&s, &mut w);

    let st = stream.physics.stats;
    let placed = stream
        .presences()
        .filter(|(id, p)| *id != player && p.position.is_some())
        .count();
    assert!(placed > 0, "the capture placed no object in the world");
    assert_eq!(
        u64::try_from(placed).expect("small"),
        st.created,
        "every placed object the server created got a body"
    );
    assert_eq!(stream.physics.len(), placed);
    assert_eq!(
        st.setup_undecodable, 0,
        "a setup record the server named will not decode"
    );
    assert_eq!(
        st.setup_not_a_setup, 0,
        "a setup id outside the 0x02000000 space"
    );
    assert_eq!(st.unplaced, 0, "an object's cell would not resolve");
    assert_eq!(st.moved, st.created, "every body was placed once");
    assert!(st.destroyed == 0);

    // The walk. `PhysicsWorld` hands its transition back to the pool at the end of every step, so
    // the counters are only readable by driving one by hand.
    let mut cell = block.cell(1);
    let mut origin = pos.frame.origin;
    landdefs::adjust_to_outside(&mut cell, &mut origin);
    let mut visited = 0_u32;
    let mut tested = 0_u32;
    for i in 1..=0x40_u32 {
        let c = CellId((u32::from(block.0) << 16) | i);
        let mut t = Transition::default();
        t.sphere_path
            .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
        let from = Position::new(c, Frame::new(Vec3::ZERO, Quat::IDENTITY));
        t.sphere_path.init_path(Some(c), Some(from), &from);
        t.sphere_path.set_check_pos(&from, Some(c));
        t.sphere_path
            .add_offset_to_check_pos(Vec3::new(0.1, 0.0, 0.0));
        let ctx = w.transition_ctx(None);
        let _ = collide::cell_find_obj_collisions(&ctx, &mut t, c);
        let _ = collide::sort_cell_find_collisions(&ctx, &mut t, c);
        assert_eq!(
            t.counters.shadow_dangling, 0,
            "cell {:#010X} keeps a dead handle",
            c.0
        );
        assert_eq!(
            t.counters.building_parts_without_bsp, 0,
            "cell {:#010X} holds an intangible shell",
            c.0
        );
        visited += t.counters.shadows_visited;
        tested += t.counters.objects_tested;
    }
    assert!(
        visited > 0,
        "no object shadows any land cell of the player's own block"
    );
    assert!(tested > 0, "the walk tested nothing");

    // At recorded logout the object model clears. Swap the earlier physics mapping into
    // that completed stream so synchronization must remove its bodies and cell registrations.
    let (mut after, _) = replay("first-login-walk-jump", usize::MAX);
    std::mem::swap(&mut after.physics, &mut stream.physics);
    after.sync_physics(&s, &mut w);
    assert!(after.physics.is_empty(), "a body outlived its object");
    assert_eq!(after.physics.stats.destroyed, st.created);
    for i in 1..=0x40_u32 {
        let c = CellId((u32::from(block.0) << 16) | i);
        let mut t = Transition::default();
        t.sphere_path
            .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
        let from = Position::new(c, Frame::new(Vec3::ZERO, Quat::IDENTITY));
        t.sphere_path.init_path(Some(c), Some(from), &from);
        t.sphere_path.set_check_pos(&from, Some(c));
        t.sphere_path
            .add_offset_to_check_pos(Vec3::new(0.1, 0.0, 0.0));
        let ctx = w.transition_ctx(None);
        let _ = collide::cell_find_obj_collisions(&ctx, &mut t, c);
        assert_eq!(
            t.counters.shadow_dangling, 0,
            "cell {:#010X} kept a dead handle",
            c.0
        );
        assert_eq!(
            t.counters.shadows_visited, 0,
            "cell {:#010X} kept a shadow",
            c.0
        );
    }
}

/// Behaviour: objects.collision.a-recorded-object-stops-a-body
/// Compare the same scripted mover with one recorded obstacle registered and with none.
/// Candidates are resident, non-ethereal outdoor objects on the player's block whose maximum
/// collision-sphere radius is at least 0.3 m; they need not be creatures.
///
/// Try up to eight bearings per candidate. The offset is `0.64 * (r_obstacle + r_body)`, the
/// oblique sphere analogue of the wall's roughly forty-degree approach. A sampled six-metre
/// corridor must remain within 0.75 m of its initial ground height. The control must approach
/// within offset + 0.05 m before the registered trial is required to stay over 0.15 m farther away.
/// The observation is the minimum horizontal distance sampled at 30 Hz over 2.5 seconds, not
/// the final position or a continuous-time minimum; sliding past is allowed. Stop after three
/// usable obstacles, but require at least one. Interior obstacles are outside this station.
#[test]
fn a_corpus_object_stops_a_body_that_would_otherwise_walk_through_it() {
    require_captures();
    let s = store();
    let (_, populated) = replay("first-login-walk-jump", usize::MAX);
    let (mut stream, _) = replay("first-login-walk-jump", populated + 1);
    let player = stream.player().expect("the corpus reaches the world");
    let ppos = stream
        .presence(player)
        .and_then(|p| p.position)
        .expect("placed");
    let block = ppos.cell.landblock();

    let src = land(&s);
    src.load_block_cells(block);
    let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
    stream.sync_physics(&s, &mut w);

    // Enumerate registered outdoor obstacles on this block; interior cases require a
    // differently placed mover and belong to the separate interior collision stations.
    let mut candidates: Vec<(ObjectId, Vec3, f32)> = Vec::new();
    for (id, p) in stream.presences() {
        let Some(pos) = p.position else { continue };
        if pos.cell.landblock() != block || !landdefs::is_outdoors(pos.cell) {
            continue;
        }
        let Some(h) = stream.physics.handle(id) else {
            continue;
        };
        let Some(o) = w.get(h) else { continue };
        if o.cell.is_none() || o.state.is_ethereal() {
            continue;
        }
        let r = o
            .geometry
            .path_spheres()
            .iter()
            .map(|q| q.radius)
            .fold(0.0_f32, f32::max);
        if r >= 0.3 {
            candidates.push((id, pos.frame.origin, r));
        }
    }
    assert!(
        !candidates.is_empty(),
        "no corpus object on the player's block has collision spheres"
    );

    /// How close the body's centre came to `at` in the horizontal plane over the whole run.
    fn closest(w: &mut PhysicsWorld, h: PhysHandle, at: Vec3, seconds: f64) -> f32 {
        let dt = 1.0 / 30.0;
        let mut t = 0.0;
        let mut best = f32::MAX;
        while t < seconds {
            t += dt;
            w.use_time(LocalTime(t), false);
            if let Some(o) = w.get(h) {
                let p = o.position.frame.origin;
                let d = (p.x - at.x)
                    .mul_add(p.x - at.x, (p.y - at.y) * (p.y - at.y))
                    .sqrt();
                best = best.min(d);
            }
        }
        best
    }

    let mut compared = 0;
    for (id, at, radius) in candidates {
        let sum = radius + 0.5;
        let offset = 0.64 * sum;
        let mut only_this: BTreeMap<ObjectId, dereth_client_runtime::objects::Presence> =
            BTreeMap::new();
        only_this.insert(id, stream.presence(id).expect("just enumerated").clone());

        for k in 0..8_i8 {
            let a = f32::from(k) * std::f32::consts::TAU / 8.0;
            let dir = Vec3::new(math::cosf(a), math::sinf(a), 0.0);
            let side = Vec3::new(-dir.y, dir.x, 0.0);
            let s0 = at.sub(dir.mul(4.0)).add(side.mul(offset));
            let Some(gz) = src.ground_height(block, s0.x, s0.y) else {
                continue;
            };
            // Screen for a roughly level ground corridor, four metres before the obstacle
            // and two beyond it. Height sampling alone does not prove unobstructed traversal;
            // the unregistered trial below supplies that additional approach control.
            let level = (0..25_i8).all(|i| {
                let q = s0.add(dir.mul(f32::from(i) * 0.25));
                src.ground_height(block, q.x, q.y)
                    .is_some_and(|z| (z - gz).abs() < 0.75)
            });
            if !level {
                continue;
            }
            let start = Vec3::new(s0.x, s0.y, gz);
            let step = dir.mul(0.12);

            let trial = |with_object: bool| -> f32 {
                let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
                if with_object {
                    let mut phys = dereth_client_runtime::object_physics::ObjectPhysics::new();
                    phys.sync(
                        &s,
                        &mut w,
                        &only_this,
                        &dereth_client_model::World::new(),
                        None,
                    );
                    assert_eq!(phys.stats.created, 1);
                    assert_eq!(phys.stats.unplaced, 0, "{:#010X} would not place", id.0);
                }
                let h = spawn2(&mut w, block, 0xDEAD_BEEF, start, step);
                closest(&mut w, h, at, 2.5)
            };

            let without = trial(false);
            // The control has to have walked through the space the object occupies, or there is
            // nothing for the object to have prevented.
            if without > offset + 0.05 {
                continue;
            }
            let with = trial(true);
            assert!(
                with > without + 0.15,
                "{:#010X}: with the object registered the body still came within {with:.3} m of \
                 it; with nothing registered it came within {without:.3} m (spheres sum to \
                 {sum:.3} m)",
                id.0
            );
            compared += 1;
            break;
        }
        if compared >= 3 {
            break;
        }
    }
    assert!(
        compared >= 1,
        "no corpus object gave a usable approach; the test measured nothing"
    );
}

/// [`spawn`] for a block other than Holtburg.
fn spawn2(
    w: &mut PhysicsWorld,
    block: LandblockId,
    id: u32,
    at: Vec3,
    per_substep: Vec3,
) -> PhysHandle {
    let h = w.create(ObjectId(id), player_geometry(), true);
    let mut cell = block.cell(1);
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

/// Behaviour: objects.carried.a-carried-object-has-no-body
/// A synthetic presence without a server position gets no body; adding a position creates
/// one. An unchanged second synchronization must not move it again. Finally, excluding this
/// object as the local player destroys the body, preventing a duplicate player body. This
/// station does not itself test removal by clearing an already populated server position.
#[test]
fn a_carried_object_has_no_body_and_a_dropped_one_gains_it() {
    let s = store();
    let src = land(&s);
    src.load_block_cells(HOLTBURG);
    let mut w = PhysicsWorld::new(Arc::clone(&src) as Arc<dyn LandSource>);
    let mut phys = dereth_client_runtime::object_physics::ObjectPhysics::new();

    let mut map: BTreeMap<ObjectId, dereth_client_runtime::objects::Presence> = BTreeMap::new();
    let id = ObjectId(0x5000_1234);
    let carried = dereth_client_runtime::objects::Presence {
        setup_id: Some(DataId(0x0200_0001)),
        scale: 1.0,
        // A presence keeps the received target (`server_position`) apart from the achieved body
        // pose (`position`, republished through `ObjectStream::publish_physics_cells`), and
        // `ObjectPhysics::sync` reads the target. A hand-built presence bypasses
        // `Presence::set_wire_position`, so it must seed both when dropped; seeding only the
        // achieved pose would create no body.
        position: None,
        server_position: None,
        ..dereth_client_runtime::objects::Presence::default()
    };
    map.insert(id, carried.clone());
    phys.sync(&s, &mut w, &map, &dereth_client_model::World::new(), None);
    assert!(
        phys.is_empty(),
        "a carried object is never handed to enter_world"
    );
    assert_eq!(phys.stats.created, 0);

    let ground = src
        .ground_height(HOLTBURG, 96.0, 96.0)
        .expect("Holtburg's terrain");
    let mut dropped = carried;
    let landed = Position::new(
        HOLTBURG.cell(1),
        Frame::new(Vec3::new(96.0, 96.0, ground), Quat::IDENTITY),
    );
    // Seeded together, exactly as `Presence::set_wire_position` seeds them for a create: at the
    // instant the wire names a position there is nothing for the two to disagree about.
    dropped.position = Some(landed);
    dropped.server_position = Some(landed);
    map.insert(id, dropped);
    phys.sync(&s, &mut w, &map, &dereth_client_model::World::new(), None);
    assert_eq!(phys.stats.created, 1);
    let h = phys.handle(id).expect("a body");
    assert!(
        w.get(h).expect("live").cell.is_some(),
        "and it is in a cell"
    );
    assert_eq!(phys.stats.unplaced, 0);

    // A second sync with nothing changed does no work: an unchanged position costs no
    // `calc_cross_cells`.
    phys.sync(&s, &mut w, &map, &dereth_client_model::World::new(), None);
    assert_eq!(phys.stats.moved, 1, "an unchanged position was re-applied");

    // The player is never given a second body.
    phys.sync(
        &s,
        &mut w,
        &map,
        &dereth_client_model::World::new(),
        Some(id),
    );
    assert!(phys.is_empty());
    assert_eq!(phys.stats.destroyed, 1);
    assert!(w.get(h).is_none(), "destroy removed it from the arena");
}
