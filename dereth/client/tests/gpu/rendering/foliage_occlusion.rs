//! Near foliage occludes what stands behind it. Landscape foliage comes in two kinds: most scenery
//! leaves are 1-bit cut-outs (`BASE1_CLIPMAP`, alpha-tested, depth written), but the low
//! broad-leafed plant (setups `0x02001063` and `0x02001064`) has blended leaf cards
//! (`BASE1_IMAGE | ALPHA`, no alpha test, no depth write), which can only be ordered by when they
//! are drawn. The world pass draws them at an alpha flush after the landscape walk has drawn every
//! outdoor object, alpha-tested list first and blended list second, so a creature behind the plant
//! is hidden and a distant tree cannot paint over the near plant.
//!
//! Fixture: the retail dats; scenery generated over the 5x5 blocks around Holtburg, and one plant
//! on block `0xA9B3` photographed on a software device with and without a creature behind it.

#![cfg(gpu)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_assets::region::Region;
use dereth_assets::world::{CellLandblock, LandblockInfo, Scene};
use dereth_assets::{decode_any, Decode, DecodedAsset};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::num::math;
use dereth_primitives::{DataId, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
use dereth_render::device::Gpu;
use dereth_render::surface::{Surface as RenderState, SurfaceHandler};
use dereth_render::{PipelineKey, SurfaceContext, VertexFormat};
use dereth_world_render::land::mesh::{generate_landblock_with_table, height_table, Direction};
use dereth_world_render::scenery::{generate_scenery, SceneryEnv};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

const W: u32 = 640;
const H: u32 = 480;

/// The block the station stands on: `0xA9B3`, immediately south of Holtburg. Holtburg itself
/// grows **no** scenery (it is a town: roads and building cells are the two
/// filters the scenery generator applies first) and that this one grows 113 pieces on flat 94 m
/// terrain. Same block, same reason.
const BLOCK: u16 = 0xA9B3;

/// **The plant.** Setup `0x02001063`, whose leaf cards are the `BASE1_IMAGE | ALPHA` surface
/// `0x08000006` — blended, no alpha test, no depth write. the scenery generator places this one at
/// block-local `(45.871, 37.346, 94.000)` in land cell `0xA9B3000A` at scale 1.888.
///
/// Every number here is re-derived from the dats by [`the_two_kinds_of_landscape_foliage`], which
/// fails if the placement has drifted from it.
const PLANT_SETUP: u32 = 0x0200_1063;
const PLANT_AT: Vec3 = Vec3::new(45.871, 37.346, 94.0);

/// The other kind: an alpha-**tested** tree, setup `0x020003D1`, 186 placements in the 5x5. Its
/// one surface `0x08000077` is `BASE1_CLIPMAP` and keeps its depth write.
const TREE_SETUP: u32 = 0x0200_03D1;

/// The creature. Setup `0x02000001`, the Aluvian male body: what matters is that it is a
/// person-sized opaque object rather than a landscape-sized one. Its furthest part stops drawing
/// at 134 m; at the five metres this station uses, every part of it draws.
const CREATURE: u32 = 0x0200_0001;
const TARGET: ObjectId = ObjectId(0x8300_0031);

/// How far in front of the plant the camera stands, and how far behind it the creature does.
const CAMERA_BACK: f32 = 0.8;
const CREATURE_BEHIND: f32 = 4.5;
/// Camera height above the terrain, chosen to look through the plant's leaf mass.
const EYE: f32 = 0.75;
/// How far east of the plant's origin the camera and the creature stand. The plant is a low
/// fern under a metre tall, its fronds splayed east and west with a gap between them over its
/// root, so the line of sight runs through the eastern frond rather than the gap.
const ACROSS: f32 = 0.7;

/// The retail store, or **fail**: a skipped test and a passing test are the same green line, so
/// this file has no skip in it.
fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

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

fn outdoor(x: f32, y: f32, z: f32) -> Position {
    let mut cell = LandblockId(BLOCK).cell(1);
    let mut o = Vec3::new(x, y, z);
    dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut o);
    Position::new(cell, dereth_primitives::Frame::new(o, Quat::IDENTITY))
}

// ---------------------------------------------------------------------------------------------
// The census — what the two kinds of foliage are, out of the dats.
// ---------------------------------------------------------------------------------------------

/// One surface of one scenery setup, as the surface-state transcription resolves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Resolved {
    surface: u32,
    surface_type: u32,
    alpha_blend: bool,
    alpha_test: bool,
    z_write: bool,
}

/// Every surface of every graphics object of one setup, run through the renderer's own
/// surface-state resolution.
fn surfaces_of(s: &RetailDatStore, setup_id: u32) -> Vec<Resolved> {
    let mut out = Vec::new();
    let Ok(b) = s.read_typed(DbType::Setup, DataId(setup_id)) else {
        return out;
    };
    let Ok(setup) =
        <dereth_assets::geometry::Setup as Decode>::decode_payload(DataId(setup_id), &b)
    else {
        return out;
    };
    for g in &setup.parts {
        let Ok(gb) = s.read_typed(DbType::GfxObj, *g) else {
            continue;
        };
        let Ok(gfx) = <dereth_assets::geometry::GfxObj as Decode>::decode_payload(*g, &gb) else {
            continue;
        };
        for sid in &gfx.surfaces {
            let Ok(sb) = s.read_typed(DbType::Surface, *sid) else {
                continue;
            };
            let Ok(surf) = <dereth_assets::material::Surface as Decode>::decode_payload(*sid, &sb)
            else {
                continue;
            };
            let state = RenderState {
                r#type: surf.surface_type,
                handler: SurfaceHandler::Database,
                color_value: surf.color_value.unwrap_or(0),
                translucency: surf.translucency,
                luminosity: surf.luminosity,
                diffuse: surf.diffuse,
            };
            // The same `SurfaceContext` the scene's own bake builds for a placed static: FVF
            // 0x152 with the normal, fixed-function lighting enabled, and a texture bound.
            let ctx = SurfaceContext {
                vertex_format: VertexFormat::XyzNormalDiffuseTex1,
                texture_is_set: true,
                lighting: true,
                ..SurfaceContext::default()
            };
            let (k, _) = PipelineKey::from_surface(&state, ctx);
            out.push(Resolved {
                surface: sid.0,
                surface_type: surf.surface_type,
                alpha_blend: k.alpha_blend,
                alpha_test: k.alpha_test,
                z_write: k.z_write,
            });
        }
    }
    out
}

/// Every scenery placement the generator grows over the 5x5 blocks around Holtburg, as
/// `(setup -> placements)`, and, separately, [`BLOCK`]'s own placements.
fn census(
    s: &RetailDatStore,
) -> (
    BTreeMap<u32, usize>,
    Vec<dereth_world_render::PlacedScenery>,
) {
    let r = region(s);
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
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut here = Vec::new();
    for bx in 0xA7..=0xABu32 {
        for by in 0xB2..=0xB6u32 {
            let id = DataId((bx << 24) | (by << 16) | 0xFFFF);
            let lb = landblock(s, id);
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
                            building_cells.insert(
                                dereth_world_render::scenery::outside_cell_index(
                                    bl.frame.origin.x,
                                    bl.frame.origin.y,
                                ),
                            );
                        }
                    }
                }
            }
            let has_building = |c: u16| building_cells.contains(&c);
            let scenes_fn = |d: DataId| load(d);
            let sphere_fn = |d: DataId| dereth_client_runtime::models::sorting_sphere(s, d);
            let env = SceneryEnv {
                scenes: &scenes_fn,
                has_building: &has_building,
                sorting_sphere: &sphere_fn,
            };
            // LINT-OK: a landblock id is two bytes, one per axis. Not a float conversion.
            #[allow(clippy::cast_possible_truncation)]
            let block = ((bx as u16) << 8) | (by as u16);
            for p in generate_scenery(&lb, &m, &r, bxi, byi, &env) {
                *counts.entry(p.gfxobj.0).or_default() += 1;
                if block == BLOCK {
                    here.push(p);
                }
            }
        }
    }
    (counts, here)
}

/// **The oracle for everything below: landscape foliage comes in two kinds, and only one of them
/// writes depth.**
#[test]
fn the_two_kinds_of_landscape_foliage() {
    let s = store();
    let (counts, here) = census(&s);
    assert!(
        counts.len() > 20,
        "only {} scenery setups in the 5x5",
        counts.len()
    );

    let mut blended_setups = 0usize;
    let mut clip_setups = 0usize;
    for setup in counts.keys() {
        let res = surfaces_of(&s, *setup);
        if res.iter().any(|r| r.alpha_blend && !r.alpha_test) {
            blended_setups += 1;
        }
        if res.iter().any(|r| r.alpha_blend && r.alpha_test) {
            clip_setups += 1;
        }
        for r in &res {
            // Catalogue rows 7-9 against rows 3-6 and 10-13: the depth write follows the alpha
            // test, and that is the fact the whole defect turns on.
            assert_eq!(
                r.z_write,
                !(r.alpha_blend && !r.alpha_test),
                "{setup:#010X} surface {:#010X} (type {:#010X}) breaks the catalogue",
                r.surface,
                r.surface_type
            );
        }
    }
    eprintln!(
        "foliage census: {} scenery setups in the 5x5 around Holtburg; {clip_setups} \
         carry an alpha-tested (depth-writing) surface, {blended_setups} carry a blended one",
        counts.len()
    );
    assert!(
        blended_setups > 0,
        "no scenery setup blends at all; the defect has no subject"
    );
    assert!(
        clip_setups > 0,
        "no scenery setup is alpha-tested; there is no control"
    );

    // The plant, named.
    let plant = surfaces_of(&s, PLANT_SETUP);
    assert!(
        plant
            .iter()
            .any(|r| r.alpha_blend && !r.alpha_test && !r.z_write),
        "{PLANT_SETUP:#010X} has no blended surface any more: {plant:?}"
    );
    // The tree, named.
    let tree = surfaces_of(&s, TREE_SETUP);
    assert!(
        tree.iter().all(|r| !r.alpha_blend || r.alpha_test),
        "{TREE_SETUP:#010X} is not purely alpha-tested any more: {tree:?}"
    );
    assert!(
        tree.iter()
            .any(|r| r.alpha_blend && r.alpha_test && r.z_write),
        "{TREE_SETUP:#010X} has no alpha-tested surface any more: {tree:?}"
    );
    eprintln!("foliage: plant {PLANT_SETUP:#010X} {plant:?}; tree {TREE_SETUP:#010X} {tree:?}");

    // And the placement the pixel station stands in front of.
    let p = here
        .iter()
        .find(|p| {
            p.gfxobj.0 == PLANT_SETUP
                && (p.frame.origin.x - PLANT_AT.x).abs() < 0.01
                && (p.frame.origin.y - PLANT_AT.y).abs() < 0.01
        })
        .unwrap_or_else(|| {
            panic!("no {PLANT_SETUP:#010X} at {PLANT_AT:?} on {BLOCK:#06X} any more")
        });
    eprintln!(
        "foliage: the station's plant is cell {:#010X} at ({:.3}, {:.3}, {:.3}) scale {:.3}",
        p.cell.0, p.frame.origin.x, p.frame.origin.y, p.frame.origin.z, p.scale
    );
    assert!(
        p.scale > 1.5,
        "the station's plant has shrunk to scale {:.3}",
        p.scale
    );
}

// ---------------------------------------------------------------------------------------------
// The driven half.
// ---------------------------------------------------------------------------------------------

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    now: f64,
    instance: u16,
}

impl Bench {
    fn new(store: &Arc<RetailDatStore>, mut gpu: Gpu) -> Self {
        // The flycam: no body, so the viewer is unambiguously outdoors and `draw_inside` cannot
        // run. A pinned time of day and no particles, so nothing moves between two captures.
        let cfg = SceneConfig {
            landblock: BLOCK,
            character: false,
            land_radius: 2,
            scenery_radius: 2,
            time_of_day: Some(0.5),
            particles: false,
            ..SceneConfig::default()
        };
        let scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
        Self {
            gpu,
            scene,
            store: store.clone(),
            objects: ObjectStream::new(),
            now: 1.0,
            instance: 0,
        }
    }

    /// Stand at `eye` looking at `at`. `FreeCamera::forward` is
    /// `(-sin yaw * cos pitch, cos yaw * cos pitch, sin pitch)`.
    fn look(&mut self, eye: Vec3, at: Vec3) {
        let d = Vec3::new(at.x - eye.x, at.y - eye.y, at.z - eye.z);
        let len = d.dot(d).sqrt().max(1.0e-6);
        let n = Vec3::new(d.x / len, d.y / len, d.z / len);
        self.scene.camera.position = eye;
        self.scene.camera.yaw = math::atan2f(-n.x, n.y);
        self.scene.camera.pitch = math::asinf(n.z);
    }

    /// One creature created at one outdoor position. A fresh `instance` each time,
    /// so re-placing the same id is accepted rather than dropped as stale.
    fn place(&mut self, at: Position) {
        self.instance += 1;
        let payload = dereth_protocol::objects::ObjectCreatePayload {
            id: TARGET,
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP,
                setup_id: Some(CREATURE),
                state: 0,
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: at.cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: at.frame.origin.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                timestamps: dereth_protocol::types::PhysicsTimestamps {
                    instance: self.instance,
                    ..dereth_protocol::types::PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        };
        let body =
            dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
                .expect("encode");
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(self.now),
        );
        self.objects.world.update_visible_object_list();
    }

    /// One frame through the app's own per-frame order.
    fn draw(&mut self) -> Vec<u8> {
        self.now += 1.0;
        let Self {
            store,
            gpu,
            scene,
            objects,
            ..
        } = self;
        scene
            .sync_objects(store, gpu, objects)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(self.now),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    }

    fn submitted(&self) -> usize {
        self.scene
            .drawn_part_order()
            .iter()
            .filter(|e| e.object == Some(TARGET))
            .count()
    }
}

fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(p, q)| p != q)
        .count()
}

/// The same differential as a screen rectangle: `(x0, y0, x1, y1, count)`. It is printed beside
/// every count so that a number can be read as a shape.
fn diff_box(a: &[u8], b: &[u8]) -> (u32, u32, u32, u32, usize) {
    let (mut x0, mut y0, mut x1, mut y1, mut n) = (W, H, 0u32, 0u32, 0usize);
    for (i, (p, q)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        if p != q {
            // LINT-OK: an index into a 640x480 frame. Not a float conversion.
            #[allow(clippy::cast_possible_truncation)]
            let (x, y) = ((i as u32) % W, (i as u32) / W);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            n += 1;
        }
    }
    (x0, y0, x1, y1, n)
}

/// One arm of the differential: which way the camera faces, and whether a creature is created.
///
/// The **clear** arm is the same camera position turned through 180 degrees with the creature the
/// same distance in front of it. That keeps the creature's range — and therefore its apparent size
/// — identical to the hidden arm's, while putting it over open ground instead of behind the
/// plant. Swinging it sideways instead cannot do both: at five metres, far enough sideways to clear
/// a plant of this size is far enough to leave the frustum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    /// Facing the plant, no creature — the backdrop [`Arm::Hidden`] is subtracted from.
    PlantOnly,
    /// Facing the plant, the creature [`CREATURE_BEHIND`] metres past it.
    Hidden,
    /// Facing away from the plant, no creature — the backdrop [`Arm::Clear`] is subtracted from.
    OpenOnly,
    /// Facing away from the plant, the creature at the same range over open ground.
    Clear,
}

impl Arm {
    const fn faces_plant(self) -> bool {
        matches!(self, Self::PlantOnly | Self::Hidden)
    }

    const fn has_creature(self) -> bool {
        matches!(self, Self::Hidden | Self::Clear)
    }
}

/// How many frames each arm runs before it is photographed, and on which of them the creature is
/// created.
const FRAMES: usize = 40;
const PLACE_ON: usize = 8;

/// One arm: its own scene, its own device, and the **same number of frames** as every other arm.
///
/// Separate scenes rather than one scene re-posed, because the in-game clock runs:
/// [`SceneConfig::time_of_day`] says where in the day a session *starts*, and the running clock
/// moves the sun from there, so two consecutive frames of one scene differ over the whole picture
/// (measured: 29,647 pixels of 307,200). Two arms photographed at the same frame index of two
/// identically driven scenes do not, and
/// [`the_plants_leaves_hide_a_creature_standing_behind_them`] asserts that noise floor before it
/// asserts anything else.
fn shot(store: &Arc<RetailDatStore>, arm: Arm) -> (Vec<u8>, usize, f32) {
    let mut b = Bench::new(store, crate::common::test_gpu(W, H));
    let plant = PLANT_AT;
    let eye = Vec3::new(plant.x + ACROSS, plant.y - CAMERA_BACK, plant.z + EYE);
    let range = CAMERA_BACK + CREATURE_BEHIND;
    // Facing the plant is +y; facing away is -y. The creature is `range` metres along whichever it
    // is, on the ground.
    let sign = if arm.faces_plant() { 1.0 } else { -1.0 };
    b.look(eye, Vec3::new(eye.x, eye.y + sign, eye.z));
    let at = Vec3::new(eye.x, eye.y + sign * range, plant.z);

    let mut frame = Vec::new();
    for i in 0..FRAMES {
        if i == PLACE_ON && arm.has_creature() {
            b.place(outdoor(at.x, at.y, at.z));
        }
        frame = b.draw();
    }
    (frame, b.submitted(), range)
}

/// Behaviour: rendering.draw-order.near-foliage-occludes-what-stands-behind-it
/// A creature standing behind the plant loses most of its visible pixels to the leaves.
#[test]
fn the_plants_leaves_hide_a_creature_standing_behind_them() {
    let store = store();

    // The control, first and on its own: two identically driven scenes with no creature in either.
    // Whatever this reports is a property of the harness, and the two arms below are read against
    // it rather than against zero.
    let (empty, _, _) = shot(&store, Arm::PlantOnly);
    let (empty_again, _, _) = shot(&store, Arm::PlantOnly);
    let floor = differing(&empty, &empty_again);
    eprintln!("foliage control: {floor} of {} pixels", W * H);
    assert_eq!(
        floor, 0,
        "two identically driven scenes do not agree; nothing below means anything"
    );

    let (hidden, hidden_subsets, range) = shot(&store, Arm::Hidden);
    let (open, _, _) = shot(&store, Arm::OpenOnly);
    let (beside, beside_subsets, _) = shot(&store, Arm::Clear);
    let hidden_pixels = differing(&empty, &hidden);
    let beside_pixels = differing(&open, &beside);

    #[allow(clippy::cast_precision_loss)] // LINT-OK: pixel counts under 307,200.
    let ratio = hidden_pixels as f32 / (beside_pixels.max(1) as f32);
    eprintln!(
        "foliage occlusion: creature at range {range:.2} m — behind the plant \
         {hidden_pixels} pixels of {hidden_subsets} subsets {:?}, in the clear {beside_pixels} \
         pixels of {beside_subsets} subsets {:?}, ratio {ratio:.3}",
        diff_box(&empty, &hidden),
        diff_box(&open, &beside)
    );

    // The premises, before the claim. Both arms must actually submit the creature, and the clear
    // arm must actually paint it: a creature degraded away or culled would give nothing over
    // nothing.
    assert!(
        hidden_subsets > 0,
        "the creature was not submitted at all behind the plant"
    );
    assert_eq!(
        hidden_subsets, beside_subsets,
        "the two arms submit different numbers of subsets, so their pixel counts are not comparable"
    );
    assert!(
        beside_pixels > 400,
        "the creature paints only {beside_pixels} pixels in the clear; the station measures nothing"
    );

    // **The claim.** Standing behind the plant costs the creature a large part of its pixels,
    // because the leaves are drawn over it.
    //
    // Measured: **0.399 with the leaves drawn at the alpha flush**, with the clear arm at 2,512
    // pixels, reproducible to the pixel, and the control above is 0 of 307,200. The threshold is
    // not a tolerance that could be widened to rescue a regression.
    //
    // It is not lower because the plant's leaves **blend**: a texel whose alpha is neither 0 nor 1
    // leaves the creature showing through it, so it still differs from the backdrop and is still
    // counted. Only the fully opaque leaf texels take the creature away, and they are a part of
    // its silhouette rather than all of it.
    assert!(
        ratio < 0.55,
        "the creature keeps {ratio:.3} of its pixels standing behind the plant's leaves \
         ({hidden_pixels} of {beside_pixels}); the leaves are not occluding it"
    );
}

/// The alpha flush draws the alpha-tested landscape list before the blended one.
///
/// It draws the alpha-tested list to exhaustion and only then the blended one, so a
/// blended batch — which writes no depth — is never laid down early enough for an alpha-tested
/// one to paint over it. `0x02001063`'s leaves are the blended kind and every tree around Holtburg
/// is the alpha-tested kind, which is why a distant tree could cover a near plant.
#[test]
fn the_clip_list_is_drawn_before_the_alpha_list() {
    let store = store();
    let mut b = Bench::new(&store, crate::common::test_gpu(W, H));
    let plant = PLANT_AT;
    let eye = Vec3::new(plant.x + ACROSS, plant.y - CAMERA_BACK, plant.z + EYE);
    b.look(eye, Vec3::new(eye.x, eye.y + 1.0, eye.z));
    for _ in 0..FRAMES {
        b.draw();
    }

    let s = b.scene.drawn_landscape_alpha();
    eprintln!(
        "foliage flush: {} alpha-tested landscape batches, {} blended drawn by the \
         frame's flush and {} by a building's, {} blended drawn before the last alpha-tested one",
        s.clip, s.blend, s.blend_early, s.blend_before_clip
    );
    // The premise: both lists have something on them, or the order below is vacuous.
    assert!(
        s.clip > 0,
        "no alpha-tested landscape batch was drawn; there is no clip list"
    );
    assert!(
        s.blend > 0,
        "no blended landscape batch was drawn; there is no alpha list"
    );
    assert_eq!(
        s.blend_before_clip, 0,
        "{} of the {} blended batches were drawn before the last of the {} alpha-tested ones; \
         the alpha flush draws the alpha-tested list first",
        s.blend_before_clip, s.blend, s.clip
    );
}
