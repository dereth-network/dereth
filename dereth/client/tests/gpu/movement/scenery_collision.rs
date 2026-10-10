//! Generated scenery takes part in the player's collision walk: a generated tree stops the body,
//! a decorative piece with no collision shape does not, and a placed collider in the same land
//! cell stops it too. Fixture: the retail dats' block `0xA9B3` south of Holtburg, a census of the
//! 5x5 blocks around it through the production scenery generator, and a local body walked with a
//! headless software GPU. All inputs come from disk or local construction; no socket is opened.
//!
//! Original landscape scenery was generated per landblock from region scene descriptions and
//! a hash of global cell coordinates, without server placement messages;
//! `dereth_world_render::scenery` implements that placement algorithm. After choosing a candidate, the
//! original client created an object with `(object_id, 0, 0)`, set its initial frame and checked
//! whether it remained inside the block. It destroyed an outside candidate. For an accepted
//! candidate it added the object to its cell, computed and applied the description's scale,
//! then appended the object to the block's static list.
//!
//! The final append tracked ownership for later destruction; it did not itself make the object
//! solid. Cell entry, the frame and cross-cell shadow registration made it available to the
//! collision walk. That walk skipped parented objects and the moving object itself, returning
//! the first non-OK collision result. Landblock-info objects followed the same create, place
//! and append pattern, resolving the outside cell from coordinates rather than the scenery
//! hash and without the generated-scenery scale operation.
//!
//! The public scenery description supplies frequency, two displacements, two scales, maximum
//! rotation, a slope band, alignment, orientation and a weenie-object flag, not a solidity flag.
//! Collision participation depends on the object's part BSP, cylinders or spheres. An object
//! with none of those shapes provides the decorative control. The census below inspects asset
//! metadata; the three walking tests separately exercise movement against selected examples.
//!
//! The scene bake carries generated placements into static-body registration, calculating
//! cross-cell coverage at the setup's own size and applying the scale afterwards. A bake that
//! drew the placements but did not register them would let the body walk through the tree while
//! the hand-registered collider control still stopped it.
//!
//! Missing retail DATs fail, and so do the scene tests when a software GPU device cannot be
//! created; the census needs no GPU. These tests inspect physics state, not rendered pixels.

#![cfg(gpu)]

use dereth_scene::world_scene::SceneWrites;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::region::Region;
use dereth_assets::world::{CellLandblock, LandblockInfo, Scene};
use dereth_assets::{decode_any, Decode, DecodedAsset};
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::{SetupGeometry, Sphere};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, DataId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use {
    dereth_client_runtime::character::CharacterInput,
    dereth_client_runtime::character::PLAYER_OBJECT_ID,
};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {
    dereth_terrain::land::mesh::generate_landblock_with_table,
    dereth_terrain::land::mesh::height_table, dereth_terrain::land::mesh::Direction,
};
use {dereth_terrain::scenery::generate_scenery, dereth_terrain::scenery::SceneryEnv};

const W: u32 = 320;
const H: u32 = 240;

/// Block immediately south of Holtburg: generated scenery on flat 94 m terrain, where the town
/// block itself has none. Roads and building cells suppress candidates, alongside the generator's bounds, slope,
/// frequency and other placement checks; they are not its only filters.
const BLOCK: u16 = 0xA9B3;

/// Selected tree setup 0x020002D9 in land cell 0xA9B3000B at (30.869,68.728,94.000): scale 1,
/// one cylinder of radius 1.530 and height 34.606, no spheres and no part BSP. The census asserts
/// this position and the first cylinder's scaled radius and prints the other shape details.
const TREE_CELL: u32 = 0xA9B3_000B;
const TREE_AT: Vec3 = Vec3::new(30.869_141, 68.728_27, 94.0);
const TREE_SETUP: u32 = 0x0200_02D9;
/// The selected first cylinder's radius after applying the placement scale.
const TREE_RADIUS: f32 = 1.530;

/// Decorative setup 0x020005AC at (36.480,60.360,94.000), scale 1, in the same
/// land cell. The census asserts its x/y location and decoded absence of spheres, cylinders
/// and part BSP; it does not assert the placement's height or scale. The walk checks passage.
const DECOR_SETUP: u32 = 0x0200_05AC;
const DECOR_AT: Vec3 = Vec3::new(36.48, 60.36, 94.0);

/// Begin 6.5 m south of the target, outside its collision shape. The positive-displacement and
/// pass-through controls below check movement over the fixed update horizon.
const APPROACH: f32 = 6.5;

/// 120 updates at 30 Hz: four simulated seconds, about 9.8 m of unobstructed walking, which is
/// well past the target. The displacement and closest-approach predicates are what is asserted.
const FRAMES: usize = 120;

/// Sampled body position and transition contact/walkable flags. Requiring both flags separates
/// stopping on walkable ground from a fall or a non-walkable contact. No pixels are read back.
#[derive(Debug, Clone, Copy)]
struct Sample {
    origin: Vec3,
    cell: u32,
    contact: bool,
    walkable: bool,
}

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    now: f64,
}

impl Bench {
    fn new(store: &Arc<RetailDatStore>, mut gpu: Gpu) -> Self {
        let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: BLOCK,
            land_radius: 2,
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(store, &region, &mut gpu)
            .expect("the body is created");
        let mut objects = ObjectStream::new();
        objects.world.player = Some(PLAYER_OBJECT_ID);
        Self {
            gpu,
            scene,
            store: store.clone(),
            objects,
            now: 1.0,
        }
    }

    /// Stand the body `APPROACH` metres south of `target`, facing north (`Quat::IDENTITY` is
    /// heading 0, which is +y), in `target`'s own land cell.
    fn stand_south_of(&mut self, cell: u32, target: Vec3) -> Vec3 {
        let at = Vec3::new(target.x, target.y - APPROACH, target.z);
        let p = Position::new(CellId(cell), Frame::new(at, Quat::IDENTITY));
        self.scene.character.as_mut().expect("a body").teleport(p);
        at
    }

    // Sync local object state and update physics; there is no draw/readback in this step.
    fn step(&mut self, input: CharacterInput) -> Sample {
        self.now += 1.0 / 30.0;
        let Self {
            store,
            gpu,
            scene,
            objects,
            now,
        } = self;
        scene
            .sync_objects(store, gpu, objects)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            input,
            LocalTime(*now),
            1.0 / 30.0,
        );
        let c = scene.character.as_ref().expect("a body");
        let o = c.world.get(c.handle).expect("the body is in the world");
        Sample {
            origin: c.position().frame.origin,
            cell: c.position().cell.0,
            contact: o.transient_state.in_contact(),
            walkable: o.transient_state.on_walkable(),
        }
    }

    // Nine zero-input updates precede the forward-input samples.
    fn settle(&mut self) -> Sample {
        let mut s = self.step(CharacterInput::default());
        for _ in 0..8 {
            s = self.step(CharacterInput::default());
        }
        s
    }

    fn walk(&mut self, frames: usize) -> Vec<Sample> {
        let input = CharacterInput {
            forward: true,
            ..CharacterInput::default()
        };
        (0..frames).map(|_| self.step(input)).collect()
    }

    fn body_radius(&self) -> f32 {
        let c = self.scene.character.as_ref().expect("a body");
        let o = c.world.get(c.handle).expect("the body");
        o.radius()
    }
}

fn bench(store: &Arc<RetailDatStore>) -> Bench {
    let gpu = crate::common::test_gpu(W, H);
    Bench::new(store, gpu)
}

/// Final horizontal displacement from the pre-settle teleport point, final position/cell,
/// minimum sampled horizontal distance to the target axis, and samples without both ground
/// flags. `travelled` is not accumulated path length; `closest` is not a continuous sweep minimum.
struct Walk {
    travelled: f32,
    end: Vec3,
    end_cell: u32,
    closest: f32,
    airborne: usize,
}

fn walk_at(b: &mut Bench, cell: u32, target: Vec3, label: &str) -> Walk {
    let start = b.stand_south_of(cell, target);
    let settled = b.settle();
    assert!(
        settled.contact && settled.walkable,
        "{label}: the body must be standing on the ground before the walk -- {settled:?}"
    );
    let path = b.walk(FRAMES);
    let last = *path.last().expect("frames were walked");
    let travelled = math::hypotf(last.origin.x - start.x, last.origin.y - start.y);
    let closest = path
        .iter()
        .map(|s| math::hypotf(s.origin.x - target.x, s.origin.y - target.y))
        .fold(f32::INFINITY, f32::min);
    let airborne = path.iter().filter(|s| !(s.contact && s.walkable)).count();
    eprintln!(
        "{label}: start [{:.3} {:.3} {:.3}] -> [{:.3} {:.3} {:.3}] cell {:#010X}, \
         travelled {travelled:.3} m, closest approach to the axis {closest:.3} m, \
         {airborne} frame(s) off the ground",
        start.x, start.y, start.z, last.origin.x, last.origin.y, last.origin.z, last.cell
    );
    Walk {
        travelled,
        end: last.origin,
        end_cell: last.cell,
        closest,
        airborne,
    }
}

// ---------------------------------------------------------------------------------------------
// The census: generated placements and recognized collision geometry in the DATs
// ---------------------------------------------------------------------------------------------

fn region(s: &RetailDatStore) -> Region {
    let id = DataId(0x1300_0000);
    let b = s.read_typed(DbType::Region, id).expect("the region record");
    match decode_any(DbType::Region, id, &b).expect("region decodes") {
        DecodedAsset::Region(r) => r,
        other => panic!("0x13000000 decoded as {other:?}"),
    }
}

fn landblock(s: &RetailDatStore, id: DataId) -> CellLandblock {
    let b = s
        .read_typed(DbType::LandBlock, id)
        .expect("a landblock record");
    match decode_any(DbType::LandBlock, id, &b).expect("landblock decodes") {
        DecodedAsset::Landblock(l) => l,
        other => panic!("{id} decoded as {other:?}"),
    }
}

/// Presence of the three recognized collision-geometry categories. The census also maps a
/// failed geometry read/decode to all-false, so that aggregate is not proof of decorative intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Arms {
    bsp: bool,
    cyl: bool,
    sphere: bool,
}

impl Arms {
    fn solid(self) -> bool {
        self.bsp || self.cyl || self.sphere
    }
}

fn geometry_of(
    s: &RetailDatStore,
    id: DataId,
    stats: &mut dereth_world_data::setup::SetupPartStats,
) -> Option<SetupGeometry> {
    if id.0 >> 24 == 0x01 {
        return dereth_world_data::setup::simple_setup_geometry(s, id, stats);
    }
    let b = s.read_typed(DbType::Setup, id).ok()?;
    let setup = dereth_assets::Setup::decode_payload(id, &b).ok()?;
    Some(dereth_world_data::setup::setup_geometry_with_parts(
        s, &setup, stats,
    ))
}

/// Inspect geometry categories over 25 blocks using the production scenery generator and
/// public asset decoders. This is not an independent placement implementation. Scene reads and
/// optional building-info decoding may fail; geometry is cached once per graphics ID, including
/// failed decodes. Simple graphics objects and setup-backed geometry share this classifier.
///
/// The sample holds both setups with collision geometry (cylinder- and sphere-backed, no part BSP)
/// and setups without any; the predicates require more than 1,000 placements and both categories,
/// not exact totals.
///
/// The last matching cell/setup placement is retained for each walking target; uniqueness is
/// not asserted. Recheck the tree's xyz and scaled first-cylinder radius, and the decoration's
/// xy and decoded lack of all three geometry categories. These selected decodes must succeed,
/// unlike the aggregate's all-false fallback. This prevents silently walking at unrelated ground
/// while preserving the narrower coordinate/shape scope of the actual assertions.
#[test]
fn the_scenery_census_says_which_pieces_retail_makes_solid() {
    let s = dereth_dat::testing::open_store_or_fail();
    let r = region(&s);
    let t = height_table(&r);
    let scene_cache: RefCell<BTreeMap<u32, Option<Scene>>> = RefCell::new(BTreeMap::new());
    let load = |id: DataId| -> Option<Scene> {
        scene_cache
            .borrow_mut()
            .entry(id.0)
            .or_insert_with(|| {
                let b = s.read_typed(DbType::Scene, id).ok()?;
                match decode_any(DbType::Scene, id, &b).ok()? {
                    DecodedAsset::Scene(sc) => Some(sc),
                    _ => None,
                }
            })
            .clone()
    };
    let mut part_stats = dereth_world_data::setup::SetupPartStats::default();
    let mut arms: BTreeMap<u32, (Arms, usize)> = BTreeMap::new();
    let mut total = 0usize;
    let mut solid_placements = 0usize;
    let mut tree: Option<dereth_terrain::scenery::PlacedScenery> = None;
    let mut decor: Option<dereth_terrain::scenery::PlacedScenery> = None;

    for bx in 0xA7..=0xABu32 {
        for by in 0xB2..=0xB6u32 {
            let id = DataId((bx << 24) | (by << 16) | 0xFFFF);
            let lb = landblock(&s, id);
            #[allow(clippy::cast_possible_wrap)] // LINT-OK: 0..=0xFE. Not a float conversion.
            let (bxi, byi) = (bx as i32, by as i32);
            let m =
                generate_landblock_with_table(&lb, &r, &t, bxi, byi, 1, Direction::InViewerBlock);
            let mut building_cells = std::collections::BTreeSet::new();
            if lb.lbi_exists != 0 {
                let info_id = DataId((id.0 & 0xFFFF_0000) | 0xFFFE);
                if let Ok(b) = s.read_typed(DbType::Lbi, info_id) {
                    if let Ok(DecodedAsset::LandblockInfo(li)) =
                        decode_any(DbType::Lbi, info_id, &b)
                    {
                        let li: LandblockInfo = li;
                        for bl in &li.buildings {
                            building_cells.insert(dereth_terrain::scenery::outside_cell_index(
                                bl.frame.origin.x,
                                bl.frame.origin.y,
                            ));
                        }
                    }
                }
            }
            let has_building = |c: u16| building_cells.contains(&c);
            let scenes_fn = |d: DataId| load(d);
            let shape_fn = |d: DataId| dereth_client_runtime::models::within_block_shape(&s, d);
            let env = SceneryEnv {
                scenes: &scenes_fn,
                has_building: &has_building,
                shape: &shape_fn,
            };
            let placed = generate_scenery(&lb, &m, &r, bxi, byi, &env);
            total += placed.len();
            for p in &placed {
                let e = arms.entry(p.gfxobj.0).or_insert_with(|| {
                    let g = geometry_of(&s, p.gfxobj, &mut part_stats);
                    let a = g.as_ref().map_or(
                        Arms {
                            bsp: false,
                            cyl: false,
                            sphere: false,
                        },
                        |g| Arms {
                            bsp: g.caches_physics_bsp(),
                            cyl: !g.cyl_spheres.is_empty(),
                            sphere: !g.spheres.is_empty(),
                        },
                    );
                    (a, 0)
                });
                e.1 += 1;
                if e.0.solid() {
                    solid_placements += 1;
                }
                if p.cell.0 == TREE_CELL && p.gfxobj.0 == TREE_SETUP {
                    tree = Some(*p);
                }
                if p.cell.0 == TREE_CELL && p.gfxobj.0 == DECOR_SETUP {
                    decor = Some(*p);
                }
            }
        }
    }

    let solid_setups = arms.values().filter(|(a, _)| a.solid()).count();
    eprintln!(
        "census over the 5x5 blocks around Holtburg: {total} scenery placements, {} distinct setup records, {solid_setups} with recognized collision geometry ({solid_placements} placements); {} records with none of the three recognized geometry branches",
        arms.len(),
        arms.len() - solid_setups
    );
    for (id, (a, n)) in &arms {
        eprintln!(
            "  {id:#010X} x{n:<5} bsp {} cyl {} sphere {} -> {}",
            u8::from(a.bsp),
            u8::from(a.cyl),
            u8::from(a.sphere),
            if a.solid() { "SOLID" } else { "decorative" }
        );
    }

    assert!(
        total > 1_000,
        "only {total} placements; the census is not looking at the world"
    );
    // Require both recognized-geometry and all-false categories; failed decodes also enter
    // the latter. The selected decoration below separately has to decode successfully.
    assert!(
        solid_setups > 0,
        "no scenery setup in the sample carries any collision geometry"
    );
    assert!(
        solid_setups < arms.len(),
        "every scenery setup in the sample is solid, so there is no decorative control to draw"
    );

    // Last matching tree placement from the shared generator; check xyz and scaled radius.
    let tree = tree.expect("the tree this file walks into must be where the scenery hash puts it");
    assert_eq!(tree.gfxobj.0, TREE_SETUP);
    assert!(
        (tree.frame.origin.x - TREE_AT.x).abs() < 0.01
            && (tree.frame.origin.y - TREE_AT.y).abs() < 0.01
            && (tree.frame.origin.z - TREE_AT.z).abs() < 0.01,
        "the tree has moved: {:?} against the file's {TREE_AT:?}",
        tree.frame.origin
    );
    let tg = geometry_of(&s, tree.gfxobj, &mut part_stats).expect("the tree's setup decodes");
    let cyl = tg
        .cyl_spheres
        .first()
        .expect("the tree is solid through the cylsphere arm");
    eprintln!(
        "tree {TREE_SETUP:#010X} at {:?} scale {:.3}: cylindrical collision shape r {:.3} h {:.3}, \
         {} spherical collision shapes, part BSP {}",
        tree.frame.origin,
        tree.scale,
        cyl.radius,
        cyl.height,
        tg.spheres.len(),
        u8::from(tg.caches_physics_bsp())
    );
    assert!(
        (cyl.radius * tree.scale - TREE_RADIUS).abs() < 0.01,
        "the tree's placed radius is {:.3}, not {TREE_RADIUS}",
        cyl.radius * tree.scale
    );

    // Decode the selected decoration; check its xy location and empty collision geometry.
    let decor = decor.expect("the decorative control must be where the scenery hash puts it");
    let dg = geometry_of(&s, decor.gfxobj, &mut part_stats).expect("its setup decodes");
    eprintln!(
        "decorative {DECOR_SETUP:#010X} at {:?} scale {:.3}: {} spherical collision shapes, {} cylindrical collision shapes, part BSP {} -- geometry used by the walking control",
        decor.frame.origin,
        decor.scale,
        dg.spheres.len(),
        dg.cyl_spheres.len(),
        u8::from(dg.caches_physics_bsp())
    );
    assert!(
        (decor.frame.origin.x - DECOR_AT.x).abs() < 0.01
            && (decor.frame.origin.y - DECOR_AT.y).abs() < 0.01,
        "the decorative control has moved: {:?}",
        decor.frame.origin
    );
    assert!(
        dg.spheres.is_empty() && dg.cyl_spheres.is_empty() && !dg.caches_physics_bsp(),
        "the control must have no spheres, cylinders or cached physics BSP"
    );
}

// ---------------------------------------------------------------------------------------------
// The walks
// ---------------------------------------------------------------------------------------------

/// Behaviour: movement.scenery.a-generated-tree-stops-the-body-and-decoration-does-not
/// Walk north toward the selected tree and require movement, grounded samples, a final point
/// short of the trunk and a sampled closest approach above the radius-sum threshold with 0.15 m
/// slack. This is one generated cylinder-backed target, not a proof for every scenery shape.
///
/// With generated land statics left unregistered, the body walks through the trunk (a closest
/// approach to the axis of millimetres, ending in the next cell north). Registered, it stops
/// 2.10 m from the axis, stepped 30 times a second, just inside 0.679 + 1.530 = 2.209 m and
/// within the 0.15 m allowance; stepped less often, in longer steps, it stops further out (2.18 m
/// at about 17 steps a second).
/// That is not an exact one-step stand-off or a continuously measured minimum.
#[test]
fn walking_into_a_generated_tree_stops_the_body() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut b = bench(&store);
    let r = b.body_radius();
    let w = walk_at(&mut b, TREE_CELL, TREE_AT, "tree 0x020002D9");

    // The instrument has to have been pointed at the tree: a body that never moved would pass
    // "it did not reach the trunk" for the wrong reason.
    assert!(
        w.travelled > 1.0,
        "the body must actually have walked at the tree -- it moved {:.3} m",
        w.travelled
    );
    assert_eq!(
        w.airborne, 0,
        "the body must stay on the ground for the whole walk"
    );
    assert_eq!(
        w.end_cell, TREE_CELL,
        "the walk stays in the tree's own land cell"
    );
    // Compare sampled separation with the two radii, allowing 0.15 m numerical/sweep slack.
    // This does not measure a continuous minimum or assert that slack equals one update step.
    assert!(
        w.closest > r + TREE_RADIUS - 0.15,
        "the tree must stop the body: closest approach was {:.3} m against a body radius {r:.3} \
         plus a trunk radius {TREE_RADIUS} = {:.3} m",
        w.closest,
        r + TREE_RADIUS
    );
    // End south of the trunk, rather than merely passing around it at a safe lateral distance.
    assert!(
        w.end.y < TREE_AT.y - TREE_RADIUS,
        "the body ended at y {:.3}, past the tree at y {:.3}",
        w.end.y,
        TREE_AT.y
    );
}

/// Pass through the selected geometry-empty decoration in the same land cell. Its y coordinate
/// differs from the tree by 8.368 m; the full horizontal separation is about 10.1 m, not 8.4 m.
/// The metadata test checks its shapes; this walk requires a sample within 0.5 m of its axis and
/// a final point more than 1 m north of it, with all samples grounded. It does not count collision
/// callbacks or establish same-frame registration. It passes whether or not generated bodies are
/// registered, since this piece has no collision shape either way.
#[test]
fn walking_into_a_decorative_scenery_piece_does_not_stop_the_body() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut b = bench(&store);
    let w = walk_at(&mut b, TREE_CELL, DECOR_AT, "decorative 0x020005AC");

    assert_eq!(
        w.airborne, 0,
        "the body must stay on the ground for the whole walk"
    );
    assert!(
        w.closest < 0.5,
        "retail walks through this piece: the closest approach was {:.3} m, so something stopped \
         the body",
        w.closest
    );
    assert!(
        w.end.y > DECOR_AT.y + 1.0,
        "the body must end past the decorative piece -- it ended at y {:.3} against its {:.3}",
        w.end.y,
        DECOR_AT.y
    );
}

/// Hand-register a static sphere collider through the same low-level create, cell-entry,
/// frame/position and cross-cell-registration pattern used for placed objects. There is no
/// server message or call through the higher-level object-spawn route in this test.
///
/// This collider stops the player whether or not generated scenery is registered, so together
/// with the tree it isolates the generated-scenery registration route; it does not prove every
/// cell-list traversal or collision primitive correct. It is a synthetic stand-in for a placed
/// object such as a fountain, not a replay of a particular one.
#[test]
fn a_server_placed_object_in_the_same_land_cell_already_stopped_the_body() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut b = bench(&store);
    // Somewhere in the same cell with nothing of its own: the cell spans x 24..48, y 48..72, and
    // the two scenery pieces above are at (30.9, 68.7) and (36.5, 60.4).
    let at = Vec3::new(28.0, 52.0, 94.0);
    const RADIUS: f32 = 1.5;
    {
        let c = b.scene.character.as_mut().expect("a body");
        let geometry = Arc::new(SetupGeometry {
            spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, RADIUS), RADIUS)],
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, RADIUS), RADIUS * 2.0),
            radius: RADIUS,
            height: 2.0 * RADIUS,
            ..SetupGeometry::default()
        });
        let h = c.world.create(ObjectId(0x7000_0001), geometry, false);
        c.world.enter_cell(h, CellId(TREE_CELL));
        if let Some(o) = c.world.get_mut(h) {
            let frame = Frame::new(at, Quat::IDENTITY);
            o.set_frame(frame);
            o.position = Position::new(CellId(TREE_CELL), frame);
        }
        c.world.calc_cross_cells(h, true);
    }
    let r = b.body_radius();
    let w = walk_at(&mut b, TREE_CELL, at, "hand-registered collider");

    // The two spheres sit at different heights — the collider's centre is `RADIUS` above its
    // origin and the body's is its own radius above its feet — so the *horizontal* separation at
    // touch is the leg of a right triangle, not the sum of the radii. The tree above is a
    // cylinder rather than an elevated sphere, so only this station uses the height correction.
    let dz = RADIUS - r;
    let touch = ((r + RADIUS).powi(2) - dz * dz).sqrt();
    eprintln!(
        "collider: body radius {r:.3}, collider radius {RADIUS}, centres {dz:.3} m apart \
         in z -> horizontal separation at touch {touch:.3} m, measured {:.3} m",
        w.closest
    );
    assert!(
        w.travelled > 1.0,
        "the body must have walked -- {:.3} m",
        w.travelled
    );
    assert_eq!(
        w.airborne, 0,
        "the body must stay on the ground for the whole walk"
    );
    assert!(
        w.closest > touch - 0.10,
        "a placed object in a land cell already stopped the body: closest approach {:.3} m \
         against a touch separation of {touch:.3} m",
        w.closest
    );
    assert!(
        w.end.y < at.y - 1.0,
        "the body ended at y {:.3}, past the collider at y {:.3}",
        w.end.y,
        at.y
    );
}
