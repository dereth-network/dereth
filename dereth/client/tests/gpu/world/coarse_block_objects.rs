//! A landblock on an outer LOD ring is bare terrain: no generated scenery, no landblock-info
//! statics and no buildings. Retail's static-object and building initialization both return unless
//! the block has eight cells per side, and demotion out of full detail releases generated objects,
//! destroys statics, then destroys buildings. `SceneConfig::lod_object_guard` is the switch; the
//! unguarded arm is the control that bakes objects on coarse blocks. The objects' absence changes
//! no pixel at the tested bearings: a coarse ring begins 288 m out, and `scenery_radius` 1 and 2
//! (a knob retail does not have) draw the same frame against a noise floor of exactly 0. Station:
//! the middle of Holtburg `0xA9B4` in a 5x5 window, aimed at the four coarse blocks carrying the
//! most objects. Fixture: the retail dats on a software device; fails without the dats or a device.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::character::CharacterInput;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::RetailDatStore;
use dereth_primitives::num::math;
use dereth_primitives::{LandblockId, LocalTime, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use std::sync::Arc;

const W: u32 = 400;
const H: u32 = 300;
const HOLTBURG: u16 = 0xA9B4;
/// A 5x5 window: ring 0 and ring 1 are full detail, ring 2 is `side_cell_count = 4`.
const LAND_RADIUS: u32 = 2;
/// Where the body stands: the middle of Holtburg's own block.
const STATION: (f32, f32) = (96.0, 96.0);

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn outdoor(x: f32, y: f32, z: f32) -> Position {
    let mut cell = LandblockId(HOLTBURG).cell(1);
    let mut o = Vec3::new(x, y, z);
    dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut o);
    Position::new(cell, dereth_primitives::Frame::new(o, Quat::IDENTITY))
}

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    now: f64,
}

impl Bench {
    fn new(store: &Arc<RetailDatStore>, scenery_radius: u32, guard: bool) -> Option<Self> {
        let mut gpu = crate::common::test_gpu(W, H);
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: HOLTBURG,
            land_radius: LAND_RADIUS,
            scenery_radius,
            time_of_day: Some(0.35),
            particles: false,
            lod_object_guard: guard,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(store, &region, &mut gpu)
            .expect("the body is created");
        Some(Self {
            gpu,
            scene,
            store: store.clone(),
            objects: ObjectStream::new(),
            now: 1.0,
        })
    }

    fn height(&self, x: f32, y: f32) -> Option<f32> {
        self.scene
            .character
            .as_ref()?
            .world
            .terrain_height_at(&outdoor(x, y, 0.0))
    }

    fn stand(&mut self, body: Vec3, yaw: f32) {
        let q = Quat::new(math::cosf(yaw * 0.5), 0.0, 0.0, math::sinf(yaw * 0.5));
        let mut p = outdoor(body.x, body.y, body.z);
        p.frame.rotation = q;
        self.scene.character.as_mut().expect("a body").teleport(p);
    }

    /// The app's own frame order, so what is captured is the production path.
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
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(self.now),
            1.0 / 30.0,
        );
        // **`App::frame`'s viewer update after the scene update.** The camera pipeline resolves
        // `CameraControl::viewer_cell` with the swept viewer and hands its resulting draw frame
        // to `scene.camera`. This file's subject is what the **LOD rings** hold, so the bearing
        // the pixels are taken on has to be the bearing retail draws.
        dereth_client::camera::update_viewer(
            scene,
            dereth_client::camera::CameraInput::default(),
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
}

fn diff(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(p, q)| p[..3] != q[..3])
        .count()
}

/// The bearings the measurement is taken on: aimed at the **coarse** blocks that carry the most
/// objects, so that "nothing changed" is a statement about geometry that is on the screen rather
/// than about an empty quarter of the sky. Chosen from the unguarded arm's own bake counts, which
/// makes the choice reproducible and independent of the guard.
fn aimed_bearings(store: &Arc<RetailDatStore>) -> Option<Vec<f32>> {
    let mut b = Bench::new(store, 2, false)?;
    let bz = b
        .height(STATION.0, STATION.1)
        .expect("the station is on terrain");
    b.stand(Vec3::new(STATION.0, STATION.1, bz + 0.01), 0.0);
    for _ in 0..4 {
        b.draw();
    }
    let viewer = b.scene.viewer_block().expect("a viewer block");
    let mut ranked: Vec<((i32, i32), usize)> = Vec::new();
    for ring in b.scene.window_rings() {
        let Some(bake) = b.scene.block_bake(ring.block) else {
            continue;
        };
        if !bake.baked || bake.side_cell_count == 8 {
            continue;
        }
        ranked.push((ring.block, bake.statics + bake.buildings * 8));
    }
    // Deterministic: by weight, then by block coordinate.
    ranked.sort_by(|a, c| c.1.cmp(&a.1).then(a.0.cmp(&c.0)));
    let mut out = Vec::new();
    for (block, weight) in ranked.into_iter().take(4) {
        if weight == 0 {
            break;
        }
        #[allow(clippy::cast_precision_loss)] // small block offsets
        let (dx, dy) = (
            ((block.0 - viewer.0) * 192) as f32 + 96.0 - STATION.0,
            ((block.1 - viewer.1) * 192) as f32 + 96.0 - STATION.1,
        );
        eprintln!("bearing at coarse block {block:?} weight {weight} ({dx:.0}, {dy:.0}) m");
        out.push(math::atan2f(-dx, dy));
    }
    Some(out)
}

/// Drive two independently built runs in lockstep at one station and return the per-bearing pixel
/// differences. Two runs, not two frames of one run: nothing in either arm is allowed to be a
/// frame the other arm never took.
fn compare(
    store: &Arc<RetailDatStore>,
    left: (u32, bool),
    right: (u32, bool),
    yaws: &[f32],
) -> Option<Vec<usize>> {
    let mut a = Bench::new(store, left.0, left.1)?;
    let mut b = Bench::new(store, right.0, right.1)?;
    let bz = a
        .height(STATION.0, STATION.1)
        .expect("the station is on terrain");
    let body = Vec3::new(STATION.0, STATION.1, bz + 0.01);
    let mut out = Vec::new();
    for &yaw in yaws {
        a.stand(body, yaw);
        b.stand(body, yaw);
        let (mut fa, mut fb) = (Vec::new(), Vec::new());
        for _ in 0..10 {
            fa = a.draw();
            fb = b.draw();
        }
        out.push(diff(&fa, &fb));
    }
    Some(out)
}

/// Behaviour: world.lod.a-coarse-landblock-bakes-no-objects
///
/// **A coarse block bakes no objects of any kind.** Retail creates no scenery, no statics and no
/// buildings on a block that is not at `side_cell_count == 8`; the client's `scenery_radius` knob
/// is what lets one be asked for. The `baked` flag is the denominator: without it "the guard
/// refused this block" and "this slot is outside the scenery radius and bakes nothing at all"
/// read identically.
#[test]
fn a_coarse_block_bakes_no_objects_of_any_kind() {
    let store = store();
    let Some(mut shipped) = Bench::new(&store, 2, true) else {
        return;
    };
    let Some(mut control) = Bench::new(&store, 2, false) else {
        return;
    };
    for b in [&mut shipped, &mut control] {
        let bz = b.height(96.0, 96.0).expect("the station is on terrain");
        b.stand(Vec3::new(96.0, 96.0, bz + 0.01), 0.0);
        for _ in 0..4 {
            b.draw();
        }
    }
    let mut checked = 0usize;
    let mut control_statics = 0usize;
    let mut control_buildings = 0usize;
    for ring in shipped.scene.window_rings() {
        let Some(s) = shipped.scene.block_bake(ring.block) else {
            continue;
        };
        if !s.baked || s.side_cell_count == 8 {
            continue;
        }
        checked += 1;
        let c = control
            .scene
            .block_bake(ring.block)
            .expect("the same block is resident on both");
        control_statics += c.statics;
        control_buildings += c.buildings;
        assert_eq!(
            (s.scenery, s.statics, s.buildings),
            (0, 0, 0),
            "block {:?} is baked at side_cell_count {} and still carries {} scenery, {} statics \
             and {} buildings. Retail's static-object and building initialization both \
             return unless side_cell_count == 8, and demotion destroys all three populations.",
            ring.block,
            s.side_cell_count,
            s.scenery,
            s.statics,
            s.buildings
        );
    }
    eprintln!(
        "guard: {checked} baked coarse blocks; the unguarded control carries {control_statics} \
         statics and {control_buildings} buildings on them"
    );
    // The denominator. A window with no coarse baked block in it would pass the loop above
    // vacuously.
    assert!(
        checked >= 8,
        "only {checked} baked coarse blocks in the window; nothing was tested"
    );
    assert!(
        control_statics + control_buildings > 0,
        "the unguarded control baked nothing on those {checked} blocks either, so the assertion \
         above is not measuring anything"
    );
}

/// **The guard changes no pixel at the tested bearings: a negative result, stated as one.**
///
/// `scenery_radius` has no counterpart in retail — its scenery radius is its whole window — so
/// with rings 0 and 1 at full detail, radius 1 and radius 2 must draw the same frame if retail's
/// guard is honoured. Aimed at the four coarse blocks carrying the most objects, **the unguarded
/// arm differs by 0 px of 120,000 on every bearing** too, against a noise floor of 0 taken first
/// from two independent runs of one configuration.
///
/// The geometry explains why. A coarse ring cannot be near: with the viewer in the middle of its
/// own block, ring 2 begins **288 m** away, and `rendering::object_draw_distance`'s census puts
/// the median static-object go-dark distance at 130 m. That median alone does not prove every
/// object is dark at 288 m; the zero-pixel differential is the evidence for the tested station
/// and bearings. Removing the extra coarse-ring objects changes counts and memory, not these
/// frames.
#[test]
fn the_correction_changes_no_pixel_at_any_reachable_station() {
    let store = store();
    let Some(yaws) = aimed_bearings(&store) else {
        return;
    };
    assert!(
        !yaws.is_empty(),
        "no coarse block in the window carries any object to look at"
    );
    // --- the noise floor, taken first ------------------------------------------------------
    let Some(noise) = compare(&store, (1, true), (1, true), &yaws) else {
        return;
    };
    eprintln!("noise floor per bearing: {noise:?} of {} px", W * H);
    for (i, n) in noise.iter().enumerate() {
        assert_eq!(
            *n, 0,
            "two independent runs of the identical configuration differ by {n} px at bearing {i}; \
             every number below would be read against noise rather than against zero"
        );
    }

    // --- the denominator: the unguarded arm really did bake more ----------------------------
    let (Some(mut one), Some(mut two)) =
        (Bench::new(&store, 1, false), Bench::new(&store, 2, false))
    else {
        return;
    };
    for b in [&mut one, &mut two] {
        let bz = b
            .height(STATION.0, STATION.1)
            .expect("the station is on terrain");
        b.stand(Vec3::new(STATION.0, STATION.1, bz + 0.01), yaws[0]);
        for _ in 0..6 {
            b.draw();
        }
    }
    let (s1, s2) = (
        one.scene.draw.stats.static_objects,
        two.scene.draw.stats.static_objects,
    );
    let (t1, t2) = (
        one.scene.draw.stats.object_triangles,
        two.scene.draw.stats.object_triangles,
    );
    eprintln!(
        "unguarded bake: scenery_radius 1 = {s1} statics / {t1} triangles, radius 2 = {s2} / {t2}"
    );
    assert!(
        s2 > s1 && t2 > t1,
        "the unguarded arm baked no extra objects at scenery_radius 2 ({s1} -> {s2} statics, \
         {t1} -> {t2} triangles), so a zero below would mean nothing"
    );

    // --- the two measurements ---------------------------------------------------------------
    let Some(pre) = compare(&store, (1, false), (2, false), &yaws) else {
        return;
    };
    let pre_total: usize = pre.iter().sum();
    eprintln!("unguarded, scenery_radius 1 vs 2, per bearing: {pre:?} = {pre_total} px");
    let Some(now) = compare(&store, (1, true), (2, true), &yaws) else {
        return;
    };
    let total: usize = now.iter().sum();
    eprintln!("shipped, scenery_radius 1 vs 2, per bearing: {now:?} = {total} px");

    assert_eq!(
        pre_total, 0,
        "the unguarded arm paints {pre_total} px (per bearing {pre:?}) of objects retail creates \
         none of. If this is ever non-zero the coarse-ring objects ARE visible and the header of \
         this file is wrong -- it says they change no pixel at these bearings."
    );
    assert_eq!(
        total, 0,
        "with the guard on, asking a coarse LOD ring for objects still changes {total} px \
         (per bearing {now:?}); the guard is not covering every population retail's demotion destroys."
    );
}
