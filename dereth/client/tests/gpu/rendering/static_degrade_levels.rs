//! A baked static draws the level of detail its own degrade record selects for its distance from
//! the camera, and the triangle count falls as the camera moves away from it.
//!
//! Three rulers, none of them a number these tests produced: the degrade lookup's distance clamp
//! (its `d` is `max(|distance| - 50, 0)`, not `(|distance| >= 50) ? distance : 0`, which the
//! whole-dat census below confirms), the shipped `GfxObjDegradeInfo` bands read from the retail
//! dats, and `dereth_world_render::objects::degrade::get_degrade` itself as the comparison for
//! what the scene drew. Fixture: Holtburg (`0xA9B4`) over the retail dats on a WARP device, with
//! no body and a fixed camera. Every fixture is an `expect`, so nothing skips.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo};
use dereth_client::character::CharacterInput;
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, LocalTime, Vec3};
use dereth_render::device::Gpu;
use dereth_world_render::objects::degrade::{get_degrade, DegradeGlobals};

/// Holtburg. The worked example throughout the knowledge base and the block every other capture
/// in this crate is taken over.
const HOLTBURG: u16 = 0xA9B4;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn warp() -> Gpu {
    crate::common::software_gpu(800, 600)
}

/// The `GfxObjDegradeInfo` a graphics object names, straight from the dat.
fn record(store: &RetailDatStore, gfxobj: DataId) -> Option<GfxObjDegradeInfo> {
    let bytes = store.read_typed(DbType::GfxObj, gfxobj).ok()?;
    let obj = GfxObj::decode_payload(gfxobj, &bytes).ok()?;
    let did = obj.did_degrade?;
    let bytes = store.read_typed(DbType::DegradeInfo, did).ok()?;
    GfxObjDegradeInfo::decode_payload(did, &bytes).ok()
}

/// A camera that does not move and a body that is not there, so that two frames of the same scene
/// differ by the thing under test and by nothing else.
///
/// `--no-character` is deliberate: with a body the camera chases it and the idle animation
/// advances, and the requirement to compare frames only when they have the same pose is
/// exactly the trap this avoids. Every frame here is drawn from a **fixed** camera with **no**
/// animating object in the scene.
fn still_scene(store: &Arc<RetailDatStore>, gpu: &mut Gpu, cfg: SceneConfig) -> WorldScene {
    WorldScene::load(store, gpu, cfg).expect("the scene loads")
}

/// One frame, with the camera parked wherever the caller put it.
fn frame(store: &Arc<RetailDatStore>, gpu: &mut Gpu, scene: &mut WorldScene, t: f64) -> Vec<u8> {
    let mut stream = ObjectStream::new();
    scene
        .sync_objects(store, gpu, &mut stream)
        .expect("sync_objects");
    scene.update(
        dereth_client::camera::CameraInput::default(),
        CharacterInput::default(),
        LocalTime(t),
        1.0 / 30.0,
    );
    scene.stream(store, gpu).expect("stream");
    scene.reserve_upload_arena(gpu).expect("reserve the arena");
    gpu.begin_frame().expect("begin");
    scene.draw(gpu).expect("draw");
    gpu.end_frame().expect("end");
    gpu.capture().expect("capture").to_rgba()
}

// ---------------------------------------------------------------------------------------------
// 1. The clamp: a census, not an argument
// ---------------------------------------------------------------------------------------------

/// Oracle: `client_portal.dat`'s whole `GfxObjDegradeInfo` space through the dat decoder.
///
/// This is the corpus half of the distance clamp in `dereth-world-render`'s `objects::degrade`.
/// `get_degrade`'s `d` reaches
/// `[0, inf)` when the degradation distance is subtracted and only `{0} u [50, inf)` when it is
/// treated as a threshold, so under the threshold reading a level whose predecessor's threshold
/// is under 50 can never be selected. Counted over the dat: **3,301 of 8,973** real levels are
/// unreachable that way against **13 of 8,973** this way. Shipped LOD data in which 37% of the
/// levels are dead is not a plausible reading of the shipped data.
#[test]
fn the_shipped_bands_are_only_reachable_if_the_degrade_distance_is_subtracted() {
    let store = store();
    let g = DegradeGlobals::default();
    assert_eq!(g.degrade_distance, 50.0, "initial degradation distance");
    assert_eq!(
        g.deg_mul, 0.0,
        "PINNED_DEG_MUL -- every band below is quoted at bias 0"
    );

    let mut seen = std::collections::HashSet::new();
    let (mut records, mut levels) = (0usize, 0usize);
    let (mut dead_threshold, mut dead_subtract) = (0usize, 0usize);
    for id in store.ids_of(DbType::GfxObj) {
        let Some(info) = record(&store, id) else {
            continue;
        };
        if !seen.insert(info.id.0) {
            continue;
        }
        if info.degrades.iter().filter(|e| e.gfxobj_id.0 != 0).count() < 2 {
            continue;
        }
        records += 1;
        // bias 0, so `threshold_i == ideal_dist_i` on both branches.
        let th: Vec<f32> = info.degrades.iter().map(|e| e.ideal_dist).collect();
        for (i, e) in info.degrades.iter().enumerate() {
            if e.gfxobj_id.0 == 0 {
                continue;
            }
            levels += 1;
            let lo = th[..i].iter().copied().fold(f32::NEG_INFINITY, f32::max);
            // Reachable iff some d in the reachable set satisfies `d >= lo` and `d < th[i]`.
            if !(th[i] > 0.0 && th[i] > lo) {
                dead_subtract += 1;
            }
            if !((lo < 0.0 && th[i] > 0.0) || th[i] > lo.max(50.0)) {
                dead_threshold += 1;
            }
        }
    }
    eprintln!(
        "{records} multi-level records, {levels} real levels: \
         unreachable under `d = distance` {dead_threshold}, under `d = max(|distance| - 50, 0)` \
         {dead_subtract}"
    );
    assert!(
        records > 2_800,
        "only {records} multi-level records; not the retail corpus"
    );
    assert_eq!(levels, 8_973);
    assert_eq!(dead_threshold, 3_301);
    assert_eq!(dead_subtract, 13);
    assert!(
        dead_threshold > 100 * dead_subtract,
        "the two readings are not distinguished by the shipped data, so this test proves nothing"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. How much of the world this is
// ---------------------------------------------------------------------------------------------

/// Oracle: the same corpus, counted by shape.
///
/// Three numbers rather than one, because "carries a record", "carries a record with more than
/// one mesh in it" and "carries a record that ends in a terminator" are three different facts,
/// and only the second is what the level switch chooses between.
#[test]
fn the_dat_says_how_many_records_can_switch_at_all() {
    let store = store();
    let ids = store.ids_of(DbType::GfxObj);
    assert!(
        ids.len() > 15_000,
        "only {} graphics objects; not the retail corpus",
        ids.len()
    );
    let mut with_record = 0usize;
    let mut hist: BTreeMap<usize, usize> = BTreeMap::new();
    let mut level0_is_self = 0usize;
    let mut with_terminator = 0usize;
    for id in &ids {
        let Some(info) = record(&store, *id) else {
            continue;
        };
        with_record += 1;
        *hist
            .entry(info.degrades.iter().filter(|e| e.gfxobj_id.0 != 0).count())
            .or_default() += 1;
        if info.degrades.first().is_some_and(|e| e.gfxobj_id == *id) {
            level0_is_self += 1;
        }
        if info.degrades.iter().any(|e| e.gfxobj_id.0 == 0) {
            with_terminator += 1;
        }
    }
    let multi: usize = hist.iter().filter(|(k, _)| **k > 1).map(|(_, v)| *v).sum();
    eprintln!(
        "{with_record} of {} graphics objects carry a record; real-level histogram {hist:?}; \
         {multi} can switch mesh; {with_terminator} can degrade to nothing; \
         level 0 names the object itself in {level0_is_self}",
        ids.len()
    );
    assert_eq!(
        with_record, 4_131,
        "graphics objects that carry a degrade record"
    );
    assert_eq!(multi, 2_891, "records with more than one real level");
    assert_eq!(hist.get(&1).copied().unwrap_or(0), 1_240);
    assert_eq!(
        with_terminator, 4_131,
        "every record ends in the FLT_MAX terminator"
    );
    // Not a rounding note: for these 229 the client's `gfxobj[0]` is a *different* mesh from the
    // part's own id, and a bake that drew `part.gfxobj` drew the wrong one even at level 0.
    assert_eq!(
        with_record - level0_is_self,
        229,
        "records whose level 0 names a mesh other than the object that points at it"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The wire: every placement draws the level `get_degrade` names, at three distances
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.degrade.a-baked-static-draws-the-level-its-distance-selects
/// Oracle: the independent degrade lookup asks the same record, at the same distance, **outside** the
/// scene.
///
/// A per-placement comparison rather than a summary: [`WorldScene::degrade_probe`] publishes the *input* each placement was measured at as
/// well as the level it drew, so the assertion re-reads the `GfxObjDegradeInfo` from the dat and
/// re-runs the lookup. A selection that agreed with itself and disagreed with the record fails.
///
/// The camera is teleported between three stations rather than walked. Nothing animates (the body
/// is absent, `character: false`), the window never scrolls (all three stations are over the same
/// point), and Holtburg's terrain is hilly enough that raising the camera does **not** move every
/// placement further away — which is why the assertions below are about agreement with the record
/// and about the level *histogram* rather than about a monotone count.
#[test]
fn every_placement_draws_the_level_get_degrade_names_at_three_distances() {
    let store = store();
    let mut gpu = warp();
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        character: false,
        scenery_radius: 1,
        ..SceneConfig::default()
    };
    let mut scene = still_scene(&store, &mut gpu, cfg);

    // A record cache, so the comparison costs one decode per distinct `GfxObjDegradeInfo`.
    let mut records: BTreeMap<u32, GfxObjDegradeInfo> = BTreeMap::new();
    let mut histograms = Vec::new();
    let mut checked = 0usize;
    // Measured rather than asserted: `get_degrade` returns a `DegradeMode` as well as a level,
    // and the draw turns modes 2-5 toward the viewer. A baked batch has fixed vertices, so
    // `select_levels` computes the mode and drops it. This counts how many of the levels the
    // scene actually selects ask for that.
    let mut billboarded = 0usize;
    // Of the resident placements, how many carry more than one *real* level (a second mesh to
    // switch to) as against a single mesh plus the terminator (which can only switch to
    // *nothing*). Both are held and picked for; only the first ever changes which triangles are
    // drawn rather than whether any are.
    let mut two_real_levels = 0usize;
    let mut one_real_level = 0usize;
    for (i, height) in [40.0f32, 160.0, 600.0].into_iter().enumerate() {
        scene.camera.position = Vec3::new(96.0, 96.0, height);
        let _ = frame(&store, &mut gpu, &mut scene, 1.0 + i as f64);
        let g = scene.degrade_globals();
        let probe = scene.degrade_probe();
        assert!(
            probe.len() > 1_000,
            "only {} placements over Holtburg",
            probe.len()
        );

        let mut hist: BTreeMap<u32, usize> = BTreeMap::new();
        for p in &probe {
            let (dist, level, id) = (&p.distance, &p.level, &p.record);
            let info = records.entry(id.0).or_insert_with(|| {
                let bytes = store
                    .read_typed(DbType::DegradeInfo, *id)
                    .expect("the record the scene is holding is in the dat");
                GfxObjDegradeInfo::decode_payload(*id, &bytes).expect("it decodes")
            });
            // LINT-OK: a level index; the longest shipped record has six. Not a float.
            #[allow(clippy::cast_possible_truncation)]
            let want = get_degrade(info, *dist, &g).0 as u32;
            assert_eq!(
                *level, want,
                "{id:?} at d = {dist}: the scene drew level {level}, get_degrade says {want}"
            );
            if i == 0 {
                let real = info.degrades.iter().filter(|e| e.gfxobj_id.0 != 0).count();
                if real > 1 {
                    two_real_levels += 1;
                } else {
                    one_real_level += 1;
                }
            }
            if info
                .degrades
                .get(*level as usize)
                .is_some_and(|e| e.degrade_mode != 1)
            {
                billboarded += 1;
            }
            *hist.entry(*level).or_default() += 1;
            checked += 1;
        }
        eprintln!(
            "z = {height:6.1} m: {} placements, levels {hist:?}, {} triangles drawn of {} held",
            probe.len(),
            scene.draw.stats.object_triangles,
            scene.draw.stats.object_triangles_resident
        );
        histograms.push(hist);
    }
    eprintln!("{checked} placement-level comparisons against get_degrade, none disagreeing");
    eprintln!(
        "{billboarded} of those {checked} selected levels ask for a billboarding mode other than \
         1, which a baked batch cannot give them"
    );
    eprintln!(
        "of the resident placements, {two_real_levels} carry a second real mesh and \
         {one_real_level} carry one mesh plus the terminator (they can only switch to nothing)"
    );
    assert!(
        two_real_levels > 0,
        "no resident placement has a second mesh to switch to"
    );
    assert_eq!(
        two_real_levels + one_real_level,
        histograms[0].values().sum::<usize>(),
        "the two classes do not add up to the placement count"
    );
    assert!(
        checked > 3_000,
        "only {checked} comparisons; the denominator is too small to mean much"
    );

    // The comparison would be vacuous if every placement always sat on level 0, so assert that
    // all three outcomes actually occur: full detail, a reduced mesh, and past the last band.
    let last = histograms.last().expect("three stations");
    assert!(
        last.len() >= 3,
        "only {} distinct levels at the far station: {last:?}",
        last.len()
    );
    assert!(
        histograms[0] != histograms[2],
        "the level histogram did not move between stations"
    );
    // Farther away, more of the block is past its last real band.
    let culled = |h: &BTreeMap<u32, usize>| -> usize {
        h.iter().filter(|(k, _)| **k >= 3).map(|(_, v)| *v).sum()
    };
    assert!(
        culled(&histograms[2]) > culled(&histograms[0]),
        "nothing extra fell past its last band at 600 m: {histograms:?}"
    );
}

/// Behaviour: rendering.degrade.a-far-land-cells-statics-are-drawn-at-the-cells-distance
/// Oracle: the land-cell grid (24 m cells from each landblock's corner) and the camera position,
/// neither of them an answer the scene computed.
///
/// Every resident outdoor placement is re-measured here from its object's origin: the centre of
/// the cell under that origin, and the horizontal offset from the camera to it. Where that offset
/// is over 50 m the placement must have been measured at exactly that horizontal distance and
/// with a level heading along it, whatever its own height or the camera's; the camera is 40 m up,
/// so a placement measured at its own or its object's distance would be off by metres. Nearer
/// cells keep their per-object measurement, which the second count shows is not the cell's.
#[test]
fn every_static_in_a_land_cell_more_than_fifty_metres_away_is_measured_at_the_cells_distance() {
    let store = store();
    let mut gpu = warp();
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        character: false,
        scenery_radius: 1,
        ..SceneConfig::default()
    };
    let mut scene = still_scene(&store, &mut gpu, cfg);
    scene.camera.position = Vec3::new(96.0, 96.0, 40.0);
    let _ = frame(&store, &mut gpu, &mut scene, 1.0);
    let centre = |v: f32| (v / 24.0).floor() * 24.0 + 12.0;
    let (mut far, mut near_own) = (0usize, 0usize);
    for p in scene.degrade_probe() {
        let Some(o) = p.object_origin else { continue };
        if !p.outdoors {
            continue;
        }
        let (dx, dy) = (centre(o.x) - p.viewer.x, centre(o.y) - p.viewer.y);
        let d = (dx * dx + dy * dy).sqrt();
        if d > 50.0 {
            far += 1;
            assert!(
                (p.cypt - d).abs() < 1e-3,
                "{o:?} in a cell {d} m away was measured at {}",
                p.cypt
            );
            let h = p.viewer_heading;
            assert!(
                (h.x - dx / d).abs() < 1e-5 && (h.y - dy / d).abs() < 1e-5 && h.z == 0.0,
                "{o:?}: heading {h:?}, the cell's is ({}, {}, 0)",
                dx / d,
                dy / d
            );
            assert!(p.shared, "{o:?} was handed a distance, not measured");
        } else if (p.cypt - d).abs() > 1.0 {
            near_own += 1;
        }
    }
    eprintln!(
        "{far} placements in far cells at the cell's distance; {near_own} nearer ones at their own"
    );
    assert!(far > 1_000, "only {far} placements in cells past 50 m");
    assert!(near_own > 0, "no nearer placement kept its own distance");
}

// ---------------------------------------------------------------------------------------------
// 4. The differential: the same walk, with and without the switch
// ---------------------------------------------------------------------------------------------

/// Oracle: two runs of the **same** scene over the **same** fixed camera stations, differing only
/// in [`SceneConfig::degrade_levels`]. The control arm is one bake, the level `get_degrade` gives
/// at `d = 0`, drawn at every distance.
///
/// The walk is a list of stations rather than a movement: the camera is placed, a frame is drawn, and the counter is
/// read. Nothing animates and the body is absent, so the two arms are in the same pose by
/// construction rather than by luck.
#[test]
fn the_triangle_count_over_a_fixed_walk_falls_with_the_switch_on() {
    let store = store();
    let mut gpu = warp();

    /// Nine stations along the diagonal of Holtburg at eye height, then three from above. Fixed,
    /// so both arms visit exactly the same points.
    const WALK: [(f32, f32, f32); 12] = [
        (16.0, 16.0, 8.0),
        (40.0, 40.0, 8.0),
        (64.0, 64.0, 8.0),
        (88.0, 88.0, 8.0),
        (112.0, 112.0, 8.0),
        (136.0, 136.0, 8.0),
        (160.0, 160.0, 8.0),
        (176.0, 176.0, 8.0),
        (96.0, 96.0, 8.0),
        (96.0, 96.0, 60.0),
        (96.0, 96.0, 160.0),
        (96.0, 96.0, 320.0),
    ];

    let mut arm = |on: bool| -> (Vec<usize>, usize, u64) {
        let cfg = SceneConfig {
            landblock: HOLTBURG,
            character: false,
            scenery_radius: 1,
            degrade_levels: on,
            ..SceneConfig::default()
        };
        let mut scene = still_scene(&store, &mut gpu, cfg);
        let mut drawn = Vec::new();
        for (i, (x, y, z)) in WALK.into_iter().enumerate() {
            scene.camera.position = Vec3::new(x, y, z);
            let _ = frame(&store, &mut gpu, &mut scene, 1.0 + i as f64);
            drawn.push(scene.draw.stats.object_triangles);
        }
        (
            drawn,
            scene.draw.stats.object_triangles_resident,
            scene.draw.stats.degrade_switches,
        )
    };

    let (before, held_before, switches_before) = arm(false);
    let (after, held_after, switches_after) = arm(true);

    eprintln!("triangles drawn per station, degrade_levels OFF: {before:?}");
    eprintln!("triangles drawn per station, degrade_levels ON : {after:?}");
    eprintln!(
        "triangles held: {held_before} -> {held_after} ({:.2}x); \
         level changes over the walk: {switches_before} -> {switches_after}",
        held_after as f64 / held_before as f64
    );

    // The control arm is a bake: its drawn count is the same at every station.
    assert!(
        before.iter().all(|t| *t == before[0]),
        "the control arm's triangle count moved, so it is not one bake: {before:?}"
    );
    assert_eq!(
        switches_before, 0,
        "the control arm switched levels, so it is not the control"
    );
    assert!(
        switches_after > 0,
        "nothing ever changed level, so the wire does not run"
    );
    // Every station draws no more than the bake did, and the far ones draw strictly fewer.
    for (i, (b, a)) in before.iter().zip(&after).enumerate() {
        assert!(
            a <= b,
            "station {i} drew more with the switch on: {a} > {b}"
        );
    }
    assert!(
        after[11] < before[11] / 2,
        "at 320 m the switch saved less than half the triangles: {} vs {}",
        after[11],
        before[11]
    );
    // And the price: every level is held, so the resident count grows.
    assert!(
        held_after > held_before,
        "no extra levels were held: {held_before} -> {held_after}"
    );
}

// ---------------------------------------------------------------------------------------------
// 5. `degrades_disabled` -- the map-mode frame
// ---------------------------------------------------------------------------------------------

/// Oracle: the degrade lookup's **first** branch — `if (degrades_disabled) { level = 0 }` —
/// raised through the client's own map-mode camera path, which disables world-object degrades.
///
/// A build that chose the level at bake time could not honour this: a map frame would draw
/// whatever the bake held, whatever the flag said. Here the flag is raised through the
/// production path — `CameraControl::set` — and every placement in the window goes back to level 0,
/// which is what "a map-mode frame draws level 0" means. It is asserted per placement rather than
/// as a pixel count because the map camera also *moves* (`viewer_offset.y = -450`), so a frame
/// pair across the flag would not be at the same station, so the same-pose rule forbids
/// reading a difference from one.
#[test]
fn map_mode_puts_every_placement_back_on_level_zero() {
    let store = store();
    let mut gpu = warp();
    let region = dereth_client::world::load_region(&store).expect("the region loads");
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        scenery_radius: 1,
        ..SceneConfig::default()
    };
    let mut scene = still_scene(&store, &mut gpu, cfg);
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    // A frame with the flag down, from wherever the chase camera puts itself.
    let _ = frame(&store, &mut gpu, &mut scene, 1.0);
    assert!(
        !scene.degrade_globals().degrades_disabled,
        "the scene raised the flag on its own"
    );
    let before = scene.degrade_probe();
    let degraded = before.iter().filter(|p| p.level != 0).count();
    eprintln!(
        "flag down: {} of {} placements past level 0, {} triangles",
        degraded,
        before.len(),
        scene.draw.stats.object_triangles
    );
    assert!(
        degraded * 2 > before.len(),
        "fewer than half the placements were degraded, so the flag has little to undo: {degraded} of {}",
        before.len()
    );
    let triangles_before = scene.draw.stats.object_triangles;

    // The client's own production map-mode call.
    {
        let c = scene.character.as_mut().expect("the body exists");
        let camera = &mut c.camera;
        let (set, manager) = (&mut camera.set, &mut camera.manager);
        set.set_map_mode(manager, true);
        assert!(
            set.effects.degrades_disabled,
            "the map-mode change did not enable degradation suppression"
        );
    }
    assert!(
        scene.degrade_globals().degrades_disabled,
        "the flag did not reach degrade_globals"
    );

    let _ = frame(&store, &mut gpu, &mut scene, 2.0);
    let after = scene.degrade_probe();
    assert_eq!(
        after.len(),
        before.len(),
        "the resident placement set moved with the flag"
    );
    let still_degraded: Vec<_> = after
        .iter()
        .filter(|p| p.level != 0)
        .map(|p| (p.level, p.record))
        .collect();
    eprintln!(
        "flag up:   {} of {} placements past level 0, {} triangles (was {triangles_before})",
        still_degraded.len(),
        after.len(),
        scene.draw.stats.object_triangles
    );
    assert!(
        still_degraded.is_empty(),
        "{} placements were still degraded in map mode: {:?}",
        still_degraded.len(),
        &still_degraded[..still_degraded.len().min(5)]
    );
    assert!(
        scene.draw.stats.object_triangles > triangles_before,
        "map mode drew no more triangles than the degraded frame: {} vs {triangles_before}",
        scene.draw.stats.object_triangles
    );
}

// ---------------------------------------------------------------------------------------------
// 6. The picture, with the differ calibrated in both directions
// ---------------------------------------------------------------------------------------------

/// Oracle: two frames of the **same** scene from the **same** fixed camera, differing only in
/// [`SceneConfig::degrade_levels`].
///
/// A zero from this instrument is useful only after it has produced a non-zero on something already
/// known to be non-zero. So the differ is
/// calibrated **in both directions before either measurement is believed**:
///
/// * a **known-different** pair — the same camera over Holtburg at two heights — must come back
///   non-zero;
/// * a **known-identical** pair — the same scene drawn twice, from the same station, with nothing
///   animating — must come back exactly zero. That is a real zero and not a file compared with
///   itself: two separate `Gpu::capture()` calls, two separate draws.
///
/// The same-pose comparison requirement is satisfied by construction: there is no pose.
/// `character: false`, so nothing in the frame animates, the camera is assigned rather than walked,
/// and the window never scrolls. The only moving part is the degrade level.
#[test]
fn the_paired_frame_changes_only_where_the_degraded_geometry_was() {
    let store = store();
    let mut gpu = warp();
    let station = Vec3::new(96.0, 96.0, 40.0);

    let mut shot = |on: bool, at: Vec3| -> Vec<u8> {
        let cfg = SceneConfig {
            landblock: HOLTBURG,
            character: false,
            scenery_radius: 1,
            degrade_levels: on,
            ..SceneConfig::default()
        };
        let mut scene = still_scene(&store, &mut gpu, cfg);
        scene.camera.position = at;
        let rgba = frame(&store, &mut gpu, &mut scene, 1.0);
        // Four scenes on one device would exhaust the 2,048-slot descriptor heap; each hands its
        // textures back before the next takes any. `release_texture` is the same call the streamer
        // makes when a block leaves the window.
        scene.release_textures(&mut gpu);
        rgba
    };

    let differ = |a: &[u8], b: &[u8]| -> usize {
        assert_eq!(a.len(), b.len(), "the two frames are not the same size");
        a.as_chunks::<4>()
            .0
            .iter()
            .zip(b.as_chunks::<4>().0.iter())
            .filter(|(p, q)| p != q)
            .count()
    };

    // Calibration, positive direction: two *different* frames must not read as identical.
    let high = shot(true, Vec3::new(96.0, 96.0, 300.0));
    let low = shot(true, station);
    let calib_pos = differ(&high, &low);
    // Calibration, negative direction: the same scene twice must read as identical.
    let low_again = shot(true, station);
    let calib_zero = differ(&low, &low_again);
    eprintln!(
        "differ calibration: known-different pair {calib_pos} px, \
         known-identical pair {calib_zero} px, of {} pixels",
        low.len() / 4
    );
    assert!(
        calib_pos > 10_000,
        "the differ read {calib_pos} px on two visibly different frames"
    );
    assert_eq!(
        calib_zero, 0,
        "the same scene drawn twice differed, so nothing below is readable"
    );

    // The measurement.
    let before = shot(false, station);
    let changed = differ(&before, &low);
    let total = low.len() / 4;
    eprintln!(
        "degrade_levels off -> on at ({}, {}, {}): {changed} of {total} pixels changed",
        station.x, station.y, station.z
    );
    assert!(changed > 0, "the switch changed nothing on screen at all");
    // Bounded from both sides. The scenery it removes is distant and thin, so it may not repaint
    // the whole frame; a change of nearly every pixel would mean something other than LOD moved.
    assert!(
        changed < total / 2,
        "{changed} of {total} pixels changed -- more than the distant scenery can account for"
    );
}

// ---------------------------------------------------------------------------------------------
// The eye the levels are measured from
// ---------------------------------------------------------------------------------------------

/// The Holtburg tavern room with the pedestal table, its stool and the long table at the far wall.
const TAVERN_ROOM: u32 = 0xA9B4_0163;
/// The stool beside the pedestal table: nearest mesh inside 4 m (3 to 8 m with the bias).
const STOOL_RECORD: u32 = 0x1100_0150;
/// The pedestal table: nearest mesh inside 4 m, the next inside 12 m.
const TABLE_RECORD: u32 = 0x1100_0170;

/// Behaviour: rendering.degrade.detail-is-measured-from-the-eye-the-frame-is-drawn-from
/// Standing in the Holtburg tavern beside the pedestal table, with the bias at 0 and adaptive
/// degrade off, every piece of furniture in the room is drawn at the level its record selects for
/// its distance from the eye the frame is drawn from, at Degrade Distance 0, 50 and 100: the stool
/// and the table, both under 4 m from the eye, keep their nearest meshes at each, as the retail
/// client draws them, where measured from the chase camera behind the eye furniture dropped a
/// level at 0.
#[test]
fn the_furniture_beside_the_player_takes_the_level_its_distance_from_the_eye_selects() {
    use dereth_primitives::{CellId, Frame, Position, Quat};
    let store = store();
    let mut gpu = warp();
    let region = dereth_client::world::load_region(&store).expect("the region loads");
    let cfg = SceneConfig {
        landblock: HOLTBURG,
        scenery_radius: 0,
        auto_degrades: false,
        ..SceneConfig::default()
    };
    let mut scene = still_scene(&store, &mut gpu, cfg);
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");
    scene.draw.cfg.render.graphics_performance = 0.0;
    // Facing north-north-east, the pedestal table ahead and to the right, its stool nearer.
    let at = Position::new(
        CellId(TAVERN_ROOM),
        Frame::new(
            Vec3::new(100.64, 37.63, 94.005),
            Quat::new(0.951, 0.0, 0.0, -0.309),
        ),
    );
    scene.character.as_mut().expect("a body").teleport(at);

    let mut stream = ObjectStream::new();
    let mut now = 0.0f64;
    for (distance, stool_level, table_level) in [(0.0, 0, 0), (50.0, 0, 0), (100.0, 0, 0)] {
        scene.draw.cfg.render.degrade_distance = distance;
        for _ in 0..6 {
            now += dereth_client::app::HEADLESS_STEP;
            scene
                .sync_objects(&store, &mut gpu, &mut stream)
                .expect("sync_objects");
            scene.update(
                dereth_client::camera::CameraInput::default(),
                CharacterInput::default(),
                LocalTime(now),
                1.0 / 30.0,
            );
            // The production camera, as the application places it after the scene update.
            dereth_client::camera::update_viewer(
                &mut scene,
                dereth_client::camera::CameraInput::default(),
                LocalTime(now),
                dereth_client::app::HEADLESS_STEP,
            );
            scene.stream(&store, &mut gpu).expect("stream");
            scene
                .reserve_upload_arena(&mut gpu)
                .expect("reserve the arena");
            gpu.begin_frame().expect("begin");
            scene.draw(&mut gpu).expect("draw");
            gpu.end_frame().expect("end");
        }
        let g = scene.degrade_globals();
        assert_eq!(
            g.degrade_distance, distance,
            "the setting reached the scene"
        );
        assert!(!g.auto_update_deg_mul && g.user_bias == 0.0);
        let eye = scene.camera.position;
        let room: Vec<_> = scene
            .degrade_probe()
            .into_iter()
            .filter(|p| !p.outdoors && p.cypt < 12.0)
            .collect();
        assert!(
            room.len() > 5,
            "the room's furniture is resident: {}",
            room.len()
        );
        let mut records: BTreeMap<u32, GfxObjDegradeInfo> = BTreeMap::new();
        for p in &room {
            // The probe measures from the eye the frame was drawn from.
            assert!(
                (p.viewer.z - eye.z).abs() < 1e-4,
                "the probe's viewer is not the eye: {:?} against {eye:?}",
                p.viewer
            );
            let info = records.entry(p.record.raw()).or_insert_with(|| {
                let bytes = store
                    .read_typed(DbType::DegradeInfo, p.record)
                    .expect("the record reads");
                GfxObjDegradeInfo::decode_payload(p.record, &bytes).expect("the record decodes")
            });
            // LINT-OK: a level index; the longest shipped record has six.
            #[allow(clippy::cast_possible_truncation)]
            let want = get_degrade(info, p.distance, &g).0 as u32;
            assert_eq!(
                p.level,
                want,
                "Degrade Distance {distance}: record {:#010X} at {:.2} m from the eye drew level \
                 {} where its distance selects {want}",
                p.record.raw(),
                p.distance,
                p.level
            );
        }
        let nearest = |record: u32| {
            room.iter()
                .filter(|p| p.record.raw() == record)
                .min_by(|a, b| a.distance.total_cmp(&b.distance))
                .unwrap_or_else(|| panic!("no {record:#010X} in the room"))
        };
        let (stool, table) = (nearest(STOOL_RECORD), nearest(TABLE_RECORD));
        eprintln!(
            "Degrade Distance {distance}: stool {:.2} m level {}, table {:.2} m level {}",
            stool.distance, stool.level, table.distance, table.level
        );
        assert_eq!(stool.level, stool_level, "the stool at {distance}");
        assert_eq!(table.level, table_level, "the table at {distance}");
    }
}
